//! Apps on wasmtime, on an engine of their own. A thread advances its epoch
//! every [`TICK`]; a call running past [`DEADLINE`] ticks traps.

use super::{Command, Event, Node, Runner};
use anyhow::{Context as _, Result, anyhow};
use guest::abi;
use std::{ops::Range, sync::OnceLock, thread, time::Duration};
use syntax::wasmtime::{
    Caller, Config, Engine, Extern, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    Trap, TypedFunc,
};

/// How often the engine's epoch moves.
const TICK: Duration = Duration::from_millis(10);
/// How many ticks one call may run.
const DEADLINE: u64 = 500;
/// The most memory one instance may hold.
const MEMORY: usize = 512 << 20;

pub struct Wasmtime {
    store: Store<Ctx>,
    memory: Memory,
    buffer: TypedFunc<u32, u32>,
    update: TypedFunc<(), ()>,
    view: TypedFunc<(), u64>,
    rows: TypedFunc<(u32, u32), u64>,
}

/// What a call's imports leave for Cydonia.
struct Ctx {
    commands: Vec<Command>,
    panic: Option<String>,
    limits: StoreLimits,
}

impl Runner for Wasmtime {
    fn start(wasm: &[u8]) -> Result<(Self, Vec<Command>)> {
        let engine = engine()?;
        let module = Module::new(engine, wasm).map_err(|error| anyhow!("{error:#}"))?;
        let mut store = Store::new(
            engine,
            Ctx {
                commands: Vec::new(),
                panic: None,
                limits: StoreLimitsBuilder::new().memory_size(MEMORY).build(),
            },
        );
        store.limiter(|ctx| &mut ctx.limits);
        store.set_epoch_deadline(DEADLINE);
        let instance = linker(engine)?
            .instantiate(&mut store, &module)
            .map_err(|error| anyhow!("{error:#}"))?;
        let memory = instance
            .get_memory(&mut store, abi::MEMORY)
            .context("The app exports no memory")?;
        let export = |store: &mut Store<Ctx>, name: &str| {
            instance
                .get_func(&mut *store, name)
                .with_context(|| format!("The app does not export {name}"))
        };
        let init = export(&mut store, abi::INIT)?
            .typed::<(), ()>(&store)
            .map_err(|error| anyhow!("{}: {error:#}", abi::INIT))?;
        let mut runner = Self {
            buffer: typed(&mut store, export, abi::BUFFER)?,
            update: typed(&mut store, export, abi::UPDATE)?,
            view: typed(&mut store, export, abi::VIEW)?,
            rows: typed(&mut store, export, abi::ROWS)?,
            store,
            memory,
        };
        runner.call(|store| init.call(store, ()))?;
        let commands = runner.commands();
        Ok((runner, commands))
    }

    fn update(&mut self, event: &Event) -> Result<Vec<Command>> {
        self.write(&serde_json::to_vec(event)?)?;
        let update = self.update.clone();
        self.call(|store| update.call(store, ()))?;
        Ok(self.commands())
    }

    fn view(&mut self) -> Result<Node> {
        let view = self.view.clone();
        let packed = self.call(|store| view.call(store, ()))?;
        serde_json::from_slice(self.read(packed)?)
            .context("The app drew a node Cydonia cannot read")
    }

    fn rows(&mut self, list: &str, range: Range<u32>) -> Result<Vec<Node>> {
        self.write(list.as_bytes())?;
        let rows = self.rows.clone();
        let packed = self.call(|store| rows.call(store, (range.start, range.end)))?;
        serde_json::from_slice(self.read(packed)?).context("The app drew a row Cydonia cannot read")
    }
}

impl Wasmtime {
    /// Run one export, with the deadline reset and a trap told as what the
    /// app was doing.
    fn call<R>(
        &mut self,
        call: impl FnOnce(&mut Store<Ctx>) -> syntax::wasmtime::Result<R>,
    ) -> Result<R> {
        self.store.set_epoch_deadline(DEADLINE);
        call(&mut self.store).map_err(|error| {
            if let Some(message) = self.store.data_mut().panic.take() {
                return anyhow!("The app panicked: {message}");
            }
            if let Some(Trap::Interrupt) = error.downcast_ref::<Trap>() {
                return anyhow!(
                    "The app ran for more than {} seconds",
                    (TICK * DEADLINE as u32).as_secs()
                );
            }
            anyhow!("The app stopped: {error:#}")
        })
    }

    /// Put `bytes` in the app's buffer, for the next call to read.
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let len = u32::try_from(bytes.len()).context("Too much to send to the app")?;
        let buffer = self.buffer.clone();
        let ptr = self.call(|store| buffer.call(store, len))?;
        self.memory
            .write(&mut self.store, ptr as usize, bytes)
            .context("The app's buffer is outside its memory")
    }

    /// The bytes an export answered with, as its pointer and length packed.
    fn read(&self, packed: u64) -> Result<&[u8]> {
        let (ptr, len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
        self.memory
            .data(&self.store)
            .get(ptr..ptr + len)
            .context("The app answered outside its memory")
    }

    fn commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.store.data_mut().commands)
    }
}

fn typed<P, R>(
    store: &mut Store<Ctx>,
    export: impl Fn(&mut Store<Ctx>, &str) -> Result<syntax::wasmtime::Func>,
    name: &str,
) -> Result<TypedFunc<P, R>>
where
    P: syntax::wasmtime::WasmParams,
    R: syntax::wasmtime::WasmResults,
{
    export(store, name)?
        .typed::<P, R>(&*store)
        .map_err(|error| anyhow!("{name}: {error:#}"))
}

/// The imports every app is offered.
fn linker(engine: &Engine) -> Result<Linker<Ctx>> {
    let mut linker = Linker::new(engine);
    linker
        .func_wrap(
            abi::MODULE,
            abi::SET_INPUT,
            |mut caller: Caller<'_, Ctx>, key: u32, key_len: u32, value: u32, value_len: u32| {
                let key = string(&mut caller, key, key_len)?;
                let value = string(&mut caller, value, value_len)?;
                caller
                    .data_mut()
                    .commands
                    .push(Command::SetInput { key, value });
                Ok(())
            },
        )
        .map_err(|error| anyhow!("{error:#}"))?;
    linker
        .func_wrap(
            abi::MODULE,
            abi::PANIC,
            |mut caller: Caller<'_, Ctx>, message: u32, len: u32| {
                let message = string(&mut caller, message, len)?;
                caller.data_mut().panic = Some(message);
                Ok(())
            },
        )
        .map_err(|error| anyhow!("{error:#}"))?;
    Ok(linker)
}

/// UTF-8 the app handed an import, out of its memory.
fn string(caller: &mut Caller<'_, Ctx>, ptr: u32, len: u32) -> syntax::wasmtime::Result<String> {
    let Some(Extern::Memory(memory)) = caller.get_export(abi::MEMORY) else {
        return Err(syntax::wasmtime::Error::msg("the app exports no memory"));
    };
    let (ptr, len) = (ptr as usize, len as usize);
    let bytes = memory
        .data(&caller)
        .get(ptr..ptr + len)
        .ok_or_else(|| syntax::wasmtime::Error::msg("a string outside the app's memory"))?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// The engine apps run on, made on first use with the thread that moves its
/// epoch.
fn engine() -> Result<&'static Engine> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config.epoch_interruption(true);
            let engine = Engine::new(&config).map_err(|error| format!("{error:#}"))?;
            let ticking = engine.weak();
            thread::Builder::new()
                .name("cydonia-apps-epoch".into())
                .spawn(move || {
                    while let Some(engine) = ticking.upgrade() {
                        engine.increment_epoch();
                        drop(engine);
                        thread::sleep(TICK);
                    }
                })
                .map_err(|error| error.to_string())?;
            Ok(engine)
        })
        .as_ref()
        .map_err(|error| anyhow!(error.clone()))
}

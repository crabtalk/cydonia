use crate::{
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::fields,
};

const NAME: Arg = Arg {
    name: "name",
    about: "The exact skill name from the built-in skill catalog.",
};

pub static TOOLS: [Tool; 1] = [Tool {
    name: "skill_read",
    description: "Read a built-in Cydonia skill. Load a relevant skill before using the workflow or syntax it describes.",
    schema: |_| fields(true, &[NAME]),
    writes: false,
    call: read,
}];

fn read(args: Args<'_>) -> Outcome {
    let name = args.text(NAME)?;
    let skill = skills::read(name).ok_or_else(|| {
        Trouble::Refused(format!(
            "no built-in skill {name}; available skills:\n{}",
            skills::catalog()
        ))
    })?;
    Ok(Answer::said(skill.content))
}

pub fn instructions() -> String {
    format!(
        "Built-in Cydonia skills:\n{}\nWhen a task matches a skill's description, call skill_read with its name before proceeding.",
        skills::catalog(),
    )
}

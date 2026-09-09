# cacp

A compact implementation of the [Agent Client Protocol][acp] v1.

ACP is how a code editor and a coding agent talk to each other — JSON-RPC over a
pipe. `cacp` implements both ends: the **agent** an editor drives, and the
**client** that drives it.

## Why

The reference implementation is large. `cacp` covers the same v1 surface — every
method, both roles, every unstable feature area — in two crates and two traits,
with no builder layer and no macros:

- **You write handlers, never a dispatch table.** Routing is a provided trait
  method; you implement the calls you serve and nothing else.
- **Every optional method declines by default.** A peer that asks for something
  you do not serve gets a clean `method not found` instead of waiting forever.
- **Notifications arrive in wire order.** The read loop awaits each one before
  it reads the next frame, so a turn's final update reaches you before its
  result does — you do not have to reassemble the ordering yourself.
- **A slow handler does not stall the stream.** Requests are spawned, so
  blocking on a permission prompt while the user makes up their mind does not
  hold up the updates queued behind it.
- **A newer peer does not break you.** Update kinds, content blocks, tool call
  content, plan payloads and enum values this revision does not know arrive in
  an `Other` variant and round-trip whole, rather than failing the message they
  came in.
- **Dropping a call cancels it.** Walk away from a request future and the peer
  gets `$/cancel_request` instead of finishing work nobody is waiting for.

## Documentation

The [book](https://crabtalk.github.io/cacp/) covers both roles, the event-loop
client and the agent registry. The API is on [docs.rs](https://docs.rs/cacp).

Build it locally with `mdbook serve docs`.

## License

[Apache-2.0](LICENSE).

[acp]: https://agentclientprotocol.com

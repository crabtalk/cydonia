//! What one prompt turn spent, read off the agent's prompt response.

use artifact::stats::Tokens;
use cacp::schema::PromptResponse;

/// The response's ACP `usage`, which the protocol defines as the whole turn.
///
/// codex-acp (1.13.1 through 2.1.1) fills it from codex's `tokenUsage.last`,
/// the turn's final model request only, so a codex turn with tool calls is
/// undercounted.
pub fn spent(response: &PromptResponse) -> Option<Tokens> {
    let usage = response.usage.as_ref()?;
    Some(Tokens {
        input: usage.input_tokens,
        output: usage.output_tokens,
        cache_read: usage.cached_read_tokens.unwrap_or(0),
        cache_write: usage.cached_write_tokens.unwrap_or(0),
    })
}

/// An estimate of a codex-acp turn: `last` (its final request, which is what
/// codex-acp sends as `usage`) scaled up to `used`, the sum of the turn's
/// usage updates — codex-acp sends one per model request, carrying that
/// request's total tokens and no breakdown. Every field scales by the same
/// ratio, so the turn is taken to split like its final request.
pub fn codex_turn(last: Tokens, used: u64) -> Tokens {
    let total = last.input + last.output + last.cache_read + last.cache_write;
    if total == 0 || used <= total {
        return last;
    }
    let ratio = used as f64 / total as f64;
    let scale = |n: u64| (n as f64 * ratio).round() as u64;
    Tokens {
        input: scale(last.input),
        output: scale(last.output),
        cache_read: scale(last.cache_read),
        cache_write: scale(last.cache_write),
    }
}

//! What a conversation's tokens would cost at the published prices of the
//! model that spent them. It is an estimate: the table is local, a model it
//! does not name is not priced, and what a subscription charges is another
//! matter.

use crate::Tokens;

/// US dollars for a million tokens of each kind.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rates {
    input: f64,
    output: f64,
    cache_read: f64,
}

const fn rates(input: f64, output: f64, cache_read: f64) -> Rates {
    Rates {
        input,
        output,
        cache_read,
    }
}

/// The models priced, by the id's family and version, without a date.
const PRICES: &[(&str, Rates)] = &[
    ("claude-fable-5-2", rates(8.0, 40.0, 0.2)),
    ("claude-mythos-5-2", rates(8.0, 40.0, 0.2)),
    ("claude-fable-5-1", rates(10.0, 50.0, 0.25)),
    ("claude-mythos-5-1", rates(10.0, 50.0, 0.25)),
    ("claude-fable-5", rates(10.0, 50.0, 1.0)),
    ("claude-mythos-5", rates(10.0, 50.0, 1.0)),
    ("claude-opus-5-5", rates(4.0, 20.0, 0.2)),
    ("claude-opus-5", rates(5.0, 25.0, 0.5)),
    ("claude-sonnet-5-5", rates(2.0, 10.0, 0.1)),
    ("claude-sonnet-5", rates(2.0, 10.0, 0.2)),
    ("claude-opus-4-8", rates(5.0, 25.0, 0.5)),
    ("claude-opus-4-7", rates(5.0, 25.0, 0.5)),
    ("claude-opus-4-6", rates(5.0, 25.0, 0.5)),
    ("claude-opus-4-5", rates(5.0, 25.0, 0.5)),
    ("claude-opus-4-1", rates(15.0, 75.0, 1.5)),
    ("claude-opus-4", rates(15.0, 75.0, 1.5)),
    ("claude-sonnet-4-6", rates(3.0, 15.0, 0.3)),
    ("claude-sonnet-4-5", rates(3.0, 15.0, 0.3)),
    ("claude-sonnet-4", rates(3.0, 15.0, 0.3)),
    ("claude-sonnet-3-7", rates(3.0, 15.0, 0.3)),
    ("claude-sonnet-3-5", rates(3.0, 15.0, 0.3)),
    ("claude-haiku-5-5", rates(0.1, 0.5, 0.01)),
    ("claude-haiku-4-5", rates(1.0, 5.0, 0.1)),
    ("claude-haiku-3-5", rates(0.8, 4.0, 0.08)),
    ("claude-haiku-3", rates(0.25, 1.25, 0.03)),
];

/// The rates of `model`: an id in the table, which may be followed by a date
/// or a context suffix, but not by the next digit of a version.
fn rates_of(model: &str) -> Option<Rates> {
    let model = model.trim().to_ascii_lowercase();
    PRICES
        .iter()
        .filter(|(key, _)| {
            model.strip_prefix(key).is_some_and(|rest| match rest {
                "" => true,
                _ => match rest.strip_prefix('-') {
                    // `-1` after `claude-opus-4` is a minor version this
                    // table does not know; `-20250514` is a date.
                    Some(more) => {
                        let digits = more.bytes().take_while(u8::is_ascii_digit).count();
                        digits == 0 || digits >= 3
                    }
                    None => !rest.starts_with(|c: char| c.is_ascii_alphanumeric()),
                },
            })
        })
        .max_by_key(|(key, _)| key.len())
        .map(|(_, rates)| *rates)
}

/// What `tokens` cost in US dollars at `model`'s prices; none for a model
/// the table does not hold. Cache reads, which `tokens` counts in its input,
/// are priced as reads, and the rest of the input as input.
pub fn estimate(model: &str, tokens: Tokens) -> Option<f64> {
    let rates = rates_of(model)?;
    let cached = tokens.cached.min(tokens.input);
    let fresh = tokens.input - cached;
    let per_million = |n: u64, rate: f64| n as f64 / 1_000_000.0 * rate;
    Some(
        per_million(fresh, rates.input)
            + per_million(cached, rates.cache_read)
            + per_million(tokens.output, rates.output),
    )
}

/// `dollars` as a person reads a small price.
pub fn show(dollars: f64) -> String {
    if dollars < 0.01 {
        "<$0.01".to_string()
    } else {
        format!("${dollars:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(input: u64, output: u64, cached: u64) -> Tokens {
        Tokens {
            input,
            output,
            cached,
        }
    }

    #[test]
    fn a_conversation_costs_its_fresh_input_cache_reads_and_output_at_the_models_prices() {
        // 1M fresh input at $5, 1M cache reads at $0.5, 1M output at $25.
        let got = estimate("claude-opus-4-7", tokens(2_000_000, 1_000_000, 1_000_000)).unwrap();
        assert!((got - 30.5).abs() < 1e-9, "{got}");
    }

    #[test]
    fn a_dated_or_suffixed_id_is_priced_as_its_model() {
        let t = tokens(1_000_000, 0, 0);
        let price = |m| estimate(m, t);
        assert_eq!(price("claude-sonnet-4-5-20250929"), Some(3.0));
        assert_eq!(price("claude-opus-4-7[1m]"), Some(5.0));
        assert_eq!(price("Claude-Opus-4-1-20250805"), Some(15.0));
        assert_eq!(price("claude-opus-4-20250514"), Some(15.0));
    }

    #[test]
    fn a_model_the_table_does_not_hold_is_not_priced() {
        let t = tokens(1_000_000, 0, 0);
        assert_eq!(estimate("model-name", t), None);
        assert_eq!(estimate("gpt-5", t), None);
        assert_eq!(estimate("", t), None);
        // A minor version not in the table is not another's price.
        assert_eq!(estimate("claude-opus-4-9", t), None);
    }

    #[test]
    fn cache_reads_beyond_the_input_are_not_priced_below_nothing() {
        let got = estimate("claude-opus-4-7", tokens(100, 0, 1_000)).unwrap();
        assert!(got >= 0.0);
    }

    #[test]
    fn a_price_is_shown_in_dollars_with_a_floor() {
        assert_eq!(show(12.345), "$12.35");
        assert_eq!(show(0.004), "<$0.01");
        assert_eq!(show(0.5), "$0.50");
    }
}

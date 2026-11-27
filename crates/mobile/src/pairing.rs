//! Pairing the phone with a work machine: the phone shows a code, the
//! operator types it on the work machine, and each side learns the other's
//! key.

use std::time::Duration;

use tsumugi_net::{Fingerprint, Identity, PairError, PairingCode};

/// How long reaching the gateway may take.
const REACH: Duration = Duration::from_secs(15);

/// How far a pairing has come, as the phone tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The work machine does not accept pairing requests now; the phone
    /// asks again each second until it does.
    NotAccepting,
    /// The operator sees the request and is to type `code`.
    Shown { code: String },
    /// The operator typed another code; the request is made again with a
    /// new one.
    Mismatched,
}

/// Ask the work machine at `address` to pair under `name`, telling `step`
/// how far it has come; the machine's key once the operator typed the code
/// shown.
pub async fn pair(
    identity: &Identity,
    address: &str,
    name: &str,
    mut step: impl FnMut(Step),
) -> Result<Fingerprint, String> {
    let mut told = None;
    let mut tell = |now: Step| {
        if told.as_ref() != Some(&now) {
            step(now.clone());
            told = Some(now);
        }
    };
    let mut code = PairingCode::generate();
    loop {
        let stream = tokio::time::timeout(REACH, tokio::net::TcpStream::connect(address))
            .await
            .map_err(|_| format!("cannot reach {address}: timed out"))?
            .map_err(|e| format!("cannot reach {address}: {e}"))?;
        let shown = Step::Shown {
            code: code.to_string(),
        };
        let asked = tsumugi_net::pair(identity, stream, name, &code, || tell(shown)).await;
        match asked {
            Ok(server) => return Ok(server),
            // No exchange ran, so the same code may be offered again.
            Err(PairError::Closed | PairError::Busy) => {
                tell(Step::NotAccepting);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            // A code is tried once; the next try asks with a new one.
            Err(PairError::Mismatch) => {
                tell(Step::Mismatched);
                code = PairingCode::generate();
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

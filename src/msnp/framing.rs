//! MSNP framing.
//!
//! The notification and switchboard servers speak CRLF-delimited commands.
//! A few commands are followed by an exact byte count of payload data, and
//! that payload may itself contain CRLF. The framer keeps partial reads until
//! a complete command (and payload) is available.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub verb: String,
    pub args: Vec<String>,
    pub payload: Option<Vec<u8>>,
}

impl Command {
    pub fn parse(line: String, payload: Option<Vec<u8>>) -> Self {
        let mut parts = line.split(' ').filter(|part| !part.is_empty());
        let verb = parts.next().unwrap_or_default().to_ascii_uppercase();
        let args = parts.map(str::to_string).collect();
        Self { verb, args, payload }
    }

    /// The transaction id, when the first argument is numeric. Server-pushed
    /// commands such as `NLN` or `FLN` do not carry one.
    pub fn trid(&self) -> Option<u32> {
        self.args.first()?.parse().ok()
    }

    /// Arguments with the leading transaction id removed.
    pub fn params(&self) -> &[String] {
        if self.trid().is_some() {
            &self.args[1..]
        } else {
            &self.args
        }
    }
}

const PAYLOAD_VERBS: &[&str] = &["MSG", "NOT", "UBX", "ADL", "RML", "NFY", "UBM", "UBN"];

#[derive(Default, Debug)]
pub struct Framer {
    buf: Vec<u8>,
}

impl Framer {
    pub fn feed(&mut self, data: &[u8]) -> Vec<Command> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();

        loop {
            let Some(line_end) = find_crlf(&self.buf) else {
                break;
            };
            let line = String::from_utf8_lossy(&self.buf[..line_end]).into_owned();

            if line.is_empty() {
                self.buf.drain(..line_end + 2);
                continue;
            }

            let verb = line
                .split(' ')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            let payload_len = if PAYLOAD_VERBS.contains(&verb.as_str()) {
                line.rsplit(' ')
                    .next()
                    .and_then(|last| last.parse::<usize>().ok())
            } else {
                None
            };

            match payload_len {
                Some(len) => {
                    let start = line_end + 2;
                    if self.buf.len() < start + len {
                        break;
                    }
                    let payload = self.buf[start..start + len].to_vec();
                    self.buf.drain(..start + len);
                    out.push(Command::parse(line, Some(payload)));
                }
                None => {
                    self.buf.drain(..line_end + 2);
                    out.push(Command::parse(line, None));
                }
            }
        }

        out
    }
}

fn find_crlf(haystack: &[u8]) -> Option<usize> {
    haystack.windows(2).position(|window| window == b"\r\n")
}

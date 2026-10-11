pub const MAX_ARGS: usize = 16;
pub const MAX_ARG_LEN: usize = 64;

/// Parsed command line representation with fixed bounds (no dynamic allocation)
pub struct ParsedArgs {
    pub args: [[u8; MAX_ARG_LEN]; MAX_ARGS],
    pub arg_lens: [usize; MAX_ARGS],
    pub count: usize,
}

impl ParsedArgs {
    pub const fn empty() -> Self {
        Self {
            args: [[0u8; MAX_ARG_LEN]; MAX_ARGS],
            arg_lens: [0usize; MAX_ARGS],
            count: 0,
        }
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        if index < self.count {
            let len = self.arg_lens[index];
            core::str::from_utf8(&self.args[index][..len]).ok()
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Parse command string into structured tokens supporting single quotes ('...'),
/// double quotes ("..."), and escape sequences.
pub fn parse_command_line(input: &str) -> ParsedArgs {
    let mut result = ParsedArgs::empty();
    let bytes = input.as_bytes();
    let mut idx = 0;
    let len = bytes.len();

    while idx < len && result.count < MAX_ARGS {
        // Skip leading whitespace
        while idx < len && (bytes[idx] == b' ' || bytes[idx] == b'\t' || bytes[idx] == b'\r' || bytes[idx] == b'\n') {
            idx += 1;
        }

        if idx >= len {
            break;
        }

        let arg_slot = result.count;
        let mut out_len = 0usize;

        while idx < len && out_len < MAX_ARG_LEN {
            let b = bytes[idx];

            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                // End of unquoted token
                break;
            } else if b == b'"' || b == b'\'' {
                let quote_char = b;
                idx += 1; // Skip opening quote
                while idx < len && bytes[idx] != quote_char {
                    if bytes[idx] == b'\\' && idx + 1 < len {
                        idx += 1; // Skip backslash
                        let escaped = bytes[idx];
                        if out_len < MAX_ARG_LEN {
                            result.args[arg_slot][out_len] = escaped;
                            out_len += 1;
                        }
                    } else if out_len < MAX_ARG_LEN {
                        result.args[arg_slot][out_len] = bytes[idx];
                        out_len += 1;
                    }
                    idx += 1;
                }
                if idx < len && bytes[idx] == quote_char {
                    idx += 1; // Skip closing quote
                }
            } else if b == b'\\' && idx + 1 < len {
                idx += 1; // Skip backslash
                let escaped = bytes[idx];
                if out_len < MAX_ARG_LEN {
                    result.args[arg_slot][out_len] = escaped;
                    out_len += 1;
                }
                idx += 1;
            } else {
                result.args[arg_slot][out_len] = b;
                out_len += 1;
                idx += 1;
            }
        }

        if out_len > 0 {
            result.arg_lens[arg_slot] = out_len;
            result.count += 1;
        }
    }

    result
}

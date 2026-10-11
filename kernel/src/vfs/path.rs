use crate::vfs::{VfsError, MAX_NAME_LEN, MAX_PATH_LEN};

/// Iterator yielding normalized components of a path string.
pub struct PathComponentIterator<'a> {
    remaining: &'a str,
}

impl<'a> PathComponentIterator<'a> {
    pub fn new(path: &'a str) -> Self {
        // Strip leading slashes
        let trimmed = path.trim_start_matches('/');
        Self { remaining: trimmed }
    }
}

impl<'a> Iterator for PathComponentIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.remaining.is_empty() {
            let next_slash = self.remaining.find('/');
            let component = match next_slash {
                Some(idx) => {
                    let comp = &self.remaining[..idx];
                    self.remaining = self.remaining[idx + 1..].trim_start_matches('/');
                    comp
                }
                None => {
                    let comp = self.remaining;
                    self.remaining = "";
                    comp
                }
            };

            if !component.is_empty() {
                return Some(component);
            }
        }
        None
    }
}

/// Normalizes an absolute path string into a fixed-capacity canonical buffer.
/// Enforces:
/// - Leading `/`
/// - Resolution of `.` (current dir)
/// - Resolution of `..` (parent dir, cannot escape root `/`)
/// - Collapsing multiple consecutive `/` into single `/`
/// - Removal of trailing `/` (except for root `/`)
pub fn normalize_path(path: &str, out: &mut [u8; MAX_PATH_LEN]) -> Result<usize, VfsError> {
    if path.is_empty() || path.len() >= MAX_PATH_LEN {
        return Err(VfsError::InvalidPath);
    }

    if !path.starts_with('/') {
        return Err(VfsError::InvalidPath); // Only absolute paths supported in root VFS
    }

    // Stack of components (up to 32 directory levels)
    let mut stack = [("", 0usize); 32];
    let mut depth = 0usize;

    for comp in PathComponentIterator::new(path) {
        if comp.len() > MAX_NAME_LEN {
            return Err(VfsError::InvalidPath);
        }

        match comp {
            "." => {
                // Ignore current directory
            }
            ".." => {
                // Pop parent directory (bounded at root)
                if depth > 0 {
                    depth -= 1;
                }
            }
            normal => {
                if depth >= stack.len() {
                    return Err(VfsError::InvalidPath);
                }
                stack[depth] = (normal, normal.len());
                depth += 1;
            }
        }
    }

    // Assemble normalized path into `out`
    let mut out_len = 0usize;
    out[0] = b'/';
    out_len += 1;

    for i in 0..depth {
        let (name, len) = stack[i];
        if out_len > 1 {
            if out_len >= MAX_PATH_LEN {
                return Err(VfsError::InvalidPath);
            }
            out[out_len] = b'/';
            out_len += 1;
        }

        if out_len + len >= MAX_PATH_LEN {
            return Err(VfsError::InvalidPath);
        }
        out[out_len..out_len + len].copy_from_slice(name.as_bytes());
        out_len += len;
    }

    Ok(out_len)
}

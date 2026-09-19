//! Validate JSON while capturing at most one bounded protocol envelope. serde's
//! reader deserializer buffers whole strings even for a custom string visitor;
//! this scanner also bounds oversized/unknown strings before deserialization.
use super::Error;
use std::io::{BufRead, BufReader, Read};

const MAX_DEPTH: usize = 128;
const READ_BUFFER_BYTES: usize = 64 * 1024;

struct Capture {
    bytes: Vec<u8>,
    overflow: bool,
}

pub(super) struct Reader<R> {
    input: BufReader<R>,
    capture: Option<Capture>,
}

impl<R: Read> Reader<R> {
    pub fn new(input: R) -> Self {
        Self {
            input: BufReader::with_capacity(READ_BUFFER_BYTES, input),
            capture: None,
        }
    }
    fn peek(&mut self) -> Result<Option<u8>, Error> {
        Ok(self.input.fill_buf()?.first().copied())
    }
    fn byte(&mut self) -> Result<u8, Error> {
        let byte = self
            .peek()?
            .ok_or(Error::Syntax("unexpected end of input"))?;
        self.input.consume(1);
        if let Some(capture) = &mut self.capture {
            if capture.bytes.len() < crate::MAX_MESSAGE_BYTES {
                capture.bytes.push(byte);
            } else {
                capture.overflow = true;
            }
        }
        Ok(byte)
    }
    fn space(&mut self) -> Result<(), Error> {
        while matches!(self.peek()?, Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.byte()?;
        }
        Ok(())
    }
    pub fn consume(&mut self, byte: u8) -> Result<bool, Error> {
        self.space()?;
        if self.peek()? == Some(byte) {
            self.byte()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub fn expect(&mut self, byte: u8) -> Result<(), Error> {
        if self.consume(byte)? {
            Ok(())
        } else {
            Err(Error::Syntax("unexpected token"))
        }
    }
    pub fn key(&mut self) -> Result<Option<String>, Error> {
        self.space()?;
        if self.peek()? != Some(b'"') {
            return Err(Error::Syntax("object key must be a string"));
        }
        self.captured_value()?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(Error::from))
            .transpose()
    }
    pub fn captured_value(&mut self) -> Result<Option<Vec<u8>>, Error> {
        self.space()?;
        self.capture = Some(Capture {
            bytes: Vec::new(),
            overflow: false,
        });
        self.value(0)?;
        let capture = self.capture.take().expect("capture started above");
        Ok((!capture.overflow).then_some(capture.bytes))
    }
    pub fn skip_value(&mut self) -> Result<(), Error> {
        self.value(0)
    }
    pub fn end(&mut self) -> Result<(), Error> {
        self.space()?;
        if self.peek()?.is_some() {
            Err(Error::Syntax("trailing data"))
        } else {
            Ok(())
        }
    }
    fn value(&mut self, depth: usize) -> Result<(), Error> {
        if depth >= MAX_DEPTH {
            return Err(Error::Syntax("JSON nesting limit"));
        }
        self.space()?;
        match self.peek()? {
            Some(b'"') => self.string(),
            Some(b'{') => {
                self.byte()?;
                if self.consume(b'}')? {
                    return Ok(());
                }
                loop {
                    self.space()?;
                    self.string()?;
                    self.expect(b':')?;
                    self.value(depth + 1)?;
                    if self.consume(b'}')? {
                        return Ok(());
                    }
                    self.expect(b',')?;
                }
            }
            Some(b'[') => {
                self.byte()?;
                if self.consume(b']')? {
                    return Ok(());
                }
                loop {
                    self.value(depth + 1)?;
                    if self.consume(b']')? {
                        return Ok(());
                    }
                    self.expect(b',')?;
                }
            }
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(Error::Syntax("expected JSON value")),
        }
    }
    fn literal(&mut self, word: &[u8]) -> Result<(), Error> {
        for &expected in word {
            if self.byte()? != expected {
                return Err(Error::Syntax("invalid literal"));
            }
        }
        Ok(())
    }
    fn digits(&mut self) -> Result<(), Error> {
        if !matches!(self.peek()?, Some(b'0'..=b'9')) {
            return Err(Error::Syntax("expected digit"));
        }
        while matches!(self.peek()?, Some(b'0'..=b'9')) {
            self.byte()?;
        }
        Ok(())
    }
    fn number(&mut self) -> Result<(), Error> {
        if self.peek()? == Some(b'-') {
            self.byte()?;
        }
        if self.peek()? == Some(b'0') {
            self.byte()?;
        } else {
            self.digits()?;
        }
        if self.peek()? == Some(b'.') {
            self.byte()?;
            self.digits()?;
        }
        if matches!(self.peek()?, Some(b'e' | b'E')) {
            self.byte()?;
            if matches!(self.peek()?, Some(b'+' | b'-')) {
                self.byte()?;
            }
            self.digits()?;
        }
        Ok(())
    }
    fn string(&mut self) -> Result<(), Error> {
        if self.byte()? != b'"' {
            return Err(Error::Syntax("expected string"));
        }
        loop {
            match self.byte()? {
                b'"' => return Ok(()),
                b'\\' => match self.byte()? {
                    b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                    b'u' => {
                        for _ in 0..4 {
                            if !self.byte()?.is_ascii_hexdigit() {
                                return Err(Error::Syntax("invalid unicode escape"));
                            }
                        }
                    }
                    _ => return Err(Error::Syntax("invalid string escape")),
                },
                0..=0x1f => return Err(Error::Syntax("unescaped control character")),
                byte @ 0x80..=0xff => {
                    let length = match byte {
                        0xc2..=0xdf => 2,
                        0xe0..=0xef => 3,
                        0xf0..=0xf4 => 4,
                        _ => return Err(Error::Syntax("invalid UTF-8")),
                    };
                    let mut sequence = [0; 4];
                    sequence[0] = byte;
                    for next in &mut sequence[1..length] {
                        *next = self.byte()?;
                    }
                    if std::str::from_utf8(&sequence[..length]).is_err() {
                        return Err(Error::Syntax("invalid UTF-8"));
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_values_without_retaining_large_unknown_fields() {
        for bytes in [
            br#"{"a":[true,false,null,-2.5e+2,"\u3042"]}"#.as_slice(),
            b"0",
            b"[]",
        ] {
            let mut input = Reader::new(bytes);
            input.skip_value().unwrap();
            input.end().unwrap();
        }
        for bytes in [
            b"01".as_slice(),
            b"1.",
            b"[1,]",
            b"{\"a\":}",
            b"\"\xff\"",
            b"\"\n\"",
            b"\"\\u000z\"",
        ] {
            let mut input = Reader::new(bytes);
            assert!(
                input.skip_value().and_then(|_| input.end()).is_err(),
                "{bytes:?}"
            );
        }
        let text = format!("\"{}\"", "x".repeat(crate::MAX_MESSAGE_BYTES * 2));
        let mut input = Reader::new(text.as_bytes());
        assert!(input.captured_value().unwrap().is_none());
        input.end().unwrap();
    }
}

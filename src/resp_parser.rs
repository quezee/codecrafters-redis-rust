use std::{num::{ParseIntError, IntErrorKind}, string::FromUtf8Error};

#[derive(Debug, PartialEq, Eq)]
enum Value {
    Str(String),
    Err(String),
    Int(i64)
}

#[derive(Debug, PartialEq, Eq)]
enum RespError {
    StartingByte{
        expected: u8,
        actual: u8
    },
    IO{
       kind: std::io::ErrorKind,
       msg: Option<String> 
    },
    FromUtf8(FromUtf8Error),
    ParseInt(IntErrorKind),
    BulkStrLen{declared: usize, actual: usize},
}

impl From<std::io::Error> for RespError {
    fn from(value: std::io::Error) -> Self {
        Self::IO { kind: value.kind(), msg: Some(value.to_string()) }
    }
}

impl From<FromUtf8Error> for RespError {
    fn from(value: FromUtf8Error) -> Self {
        Self::FromUtf8(value)
    }
}

impl From<ParseIntError> for RespError {
    fn from(value: ParseIntError) -> Self {
        Self::ParseInt(*value.kind())
    }
}

trait Parser {
    const STARTING_BYTE: u8;

    fn deserialize<R: std::io::BufRead>(reader: &mut R) -> Result<Value, RespError>;

    fn check_starting_byte<R: std::io::BufRead>(reader: &mut R, control_byte: &mut [u8]) -> Result<(), RespError> {
        reader.read_exact(control_byte)?;
        if control_byte[0] != Self::STARTING_BYTE {
            return Err(RespError::StartingByte{expected: Self::STARTING_BYTE, actual: control_byte[0]});
        }
        Ok(())
    }

    fn read_until_crlf<R: std::io::BufRead>(reader: &mut R, control_byte: &mut [u8]) -> Result<Vec<u8>, RespError> {
        let mut buf = vec![];

        reader.read_until(b'\r', &mut buf)?;
        reader.read_exact(control_byte)?;

        while control_byte[0] != b'\n' {
            buf.extend_from_slice(control_byte);
            if control_byte[0] == b'\r' {
                reader.read_exact(control_byte)?;
                continue;
            }
            reader.read_until(b'\r', &mut buf)?;
            reader.read_exact(control_byte)?;
        }
        buf.pop();
        Ok(buf)
    }

    fn deserialize_str<R: std::io::BufRead>(reader: &mut R) -> Result<String, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte)?;

        let buf = Self::read_until_crlf(reader, &mut control_byte)?;
        let result = String::from_utf8(buf)?;
        Ok(result)
    }
}

struct StrParser;
impl Parser for StrParser {
    const STARTING_BYTE: u8 = b'+';

    fn deserialize<R: std::io::BufRead>(reader: &mut R) -> Result<Value, RespError> {
        let result = Self::deserialize_str(reader)?;
        Ok(Value::Str(result))
    }
}

struct ErrParser;
impl Parser for ErrParser {
    const STARTING_BYTE: u8 = b'-';

    fn deserialize<R: std::io::BufRead>(reader: &mut R) -> Result<Value, RespError> {
        let result = Self::deserialize_str(reader)?;
        Ok(Value::Err(result))
    }
}

struct IntParser;
impl Parser for IntParser {
    const STARTING_BYTE: u8 = b':';

    fn deserialize<R: std::io::BufRead>(reader: &mut R) -> Result<Value, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte)?;

        let buf = Self::read_until_crlf(reader, &mut control_byte)?;
        let buf_str = String::from_utf8(buf)?;
        let result = buf_str.parse::<i64>()?;
        Ok(Value::Int(result))
    }
}

struct BulkStrParser;
impl Parser for BulkStrParser {
    const STARTING_BYTE: u8 = b'$';

    fn deserialize<R: std::io::BufRead>(reader: &mut R) -> Result<Value, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte)?;

        let buf = Self::read_until_crlf(reader, &mut control_byte)?;
        let buf_str = String::from_utf8(buf)?;
        let str_len = buf_str.parse::<usize>()?;
        
        let buf = Self::read_until_crlf(reader, &mut control_byte)?;
        if buf.len() != str_len {
            return Err(RespError::BulkStrLen{declared: str_len, actual: buf.len()});
        }
        let result = String::from_utf8(buf)?;
        Ok(Value::Str(result))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_str_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"-wrong start\r\n");
            let result = StrParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b'+', actual: b'-' });
        }
        {   // simple case
            let mut cursor = Cursor::new(b"+simple case\r\n");
            let result = StrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("simple case".into()));
        }
        {   // CR in the middle and in the end
            let mut cursor = Cursor::new(b"+with \r in the end\r\r\n");
            let result = StrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("with \r in the end\r".into()));
        }
        {   // LF in the middle and in the end
            let mut cursor = Cursor::new(b"+with \n in the end\n\r\n");
            let result = StrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("with \n in the end\n".into()));
        }
    }

    #[test]
    fn test_err_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"+wrong start\r\n");
            let result = ErrParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b'-', actual: b'+' });
        }
        {   // simple case
            let mut cursor = Cursor::new(b"-simple case\r\n");
            let result = ErrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Err("simple case".into()));
        }
    }

    #[test]
    fn test_int_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"+123\r\n");
            let result = IntParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b':', actual: b'+' });
        }
        {   // non-numerical symbol
            let mut cursor = Cursor::new(b":1a3\r\n");
            let result = IntParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::ParseInt(IntErrorKind::InvalidDigit));
        }
        {   // correct int
            let mut cursor = Cursor::new(b":123\r\n");
            let result = IntParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Int(123));
        }
    }

    #[test]
    fn test_bulk_str_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"+11\r\nwrong start\r\n");
            let result = BulkStrParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b'$', actual: b'+' });
        }
        {   // wrong declared len
            let mut cursor = Cursor::new(b"$10\r\nwrong len\r\n");
            let result = BulkStrParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::BulkStrLen { declared: 10, actual: 9 });
        }
        {   // correct case
            let mut cursor = Cursor::new(b"$14\r\ncorrect\ncase\n\r\r\n");
            let result = BulkStrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("correct\ncase\n\r".into()));
        }
    }

}
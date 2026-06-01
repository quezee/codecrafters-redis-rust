use std::{io, num::{ParseIntError, IntErrorKind}, string::FromUtf8Error};
use crate::resp_value::Value;


#[derive(Debug, PartialEq, Eq)]
pub enum RespError {
    StartingByte{
        expected: u8,
        actual: u8
    },
    UnknownStartingByte(u8),
    IO{
       kind: io::ErrorKind,
       msg: Option<String> 
    },
    FromUtf8(FromUtf8Error),
    ParseInt(IntErrorKind),
    WrongNullValue([u8; 4]),
    WrongBulkStrEnding([u8; 2]),
}

impl From<io::Error> for RespError {
    fn from(value: io::Error) -> Self {
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

pub trait Parser {
    const STARTING_BYTE: u8;

    fn deserialize<R: io::BufRead>(reader: &mut R) -> Result<Value, RespError>;

    fn check_starting_byte<R: io::BufRead>(reader: &mut R, control_byte: &mut [u8]) -> Result<(), RespError> {
        reader.read_exact(control_byte)?;
        if control_byte[0] != Self::STARTING_BYTE {
            return Err(RespError::StartingByte{expected: Self::STARTING_BYTE, actual: control_byte[0]});
        }
        Ok(())
    }

    fn peek_next_byte<R: io::BufRead>(reader: &mut R) -> io::Result<u8> {
        let buf = reader.fill_buf()?;
        match buf.first() {
            Some(byte) => Ok(*byte),
            None => Err(io::Error::from(io::ErrorKind::UnexpectedEof))
        }
    }

    fn read_until_crlf<R: io::BufRead>(reader: &mut R, control_byte: &mut [u8]) -> Result<Vec<u8>, RespError> {
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

    fn deserialize_str<R: io::BufRead>(reader: &mut R) -> Result<String, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte)?;

        let buf = Self::read_until_crlf(reader, &mut control_byte)?;
        let result = String::from_utf8(buf)?;
        Ok(result)
    }

    fn deserialize_len<R: io::BufRead>(reader: &mut R, control_byte: &mut [u8]) -> Result<usize, RespError> {
        let buf = Self::read_until_crlf(reader, control_byte)?;
        let buf_str = String::from_utf8(buf)?;
        let len = buf_str.parse::<usize>()?;
        Ok(len)
    }
}

pub struct StrParser;
impl Parser for StrParser {
    const STARTING_BYTE: u8 = b'+';

    fn deserialize<R: io::BufRead>(reader: &mut R) -> Result<Value, RespError> {
        let result = Self::deserialize_str(reader)?;
        Ok(Value::Str(result))
    }
}

pub struct ErrParser;
impl Parser for ErrParser {
    const STARTING_BYTE: u8 = b'-';

    fn deserialize<R: io::BufRead>(reader: &mut R) -> Result<Value, RespError> {
        let result = Self::deserialize_str(reader)?;
        Ok(Value::Err(result))
    }
}

pub struct IntParser;
impl Parser for IntParser {
    const STARTING_BYTE: u8 = b':';

    fn deserialize<R: io::BufRead>(reader: &mut R) -> Result<Value, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte)?;

        let buf = Self::read_until_crlf(reader, &mut control_byte)?;
        let buf_str = String::from_utf8(buf)?;
        let result = buf_str.parse::<i64>()?;
        Ok(Value::Int(result))
    }
}

pub struct BulkStrParser;
impl Parser for BulkStrParser {
    const STARTING_BYTE: u8 = b'$';

    fn deserialize<R: io::BufRead>(reader: &mut R) -> Result<Value, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte)?;

        let next_byte = Self::peek_next_byte(reader)?;
        if next_byte == b'-' {
            // null value processing (`-1\r\n` expected)
            let mut null_val_buf = [0u8; 4];
            reader.read_exact(&mut null_val_buf)?;
            if &null_val_buf == b"-1\r\n" {
                return Ok(Value::Null);
            } else {
                return Err(RespError::WrongNullValue(null_val_buf));
            }
        }

        let str_len = Self::deserialize_len(reader, &mut control_byte)?;
        let mut buf = vec![0u8; str_len];
        reader.read_exact(&mut buf)?;

        let mut ending_bytes = [0u8; 2];
        reader.read_exact(&mut ending_bytes)?;
        if &ending_bytes != b"\r\n" {
            return Err(RespError::WrongBulkStrEnding(ending_bytes));
        }

        let result = String::from_utf8(buf)?;
        Ok(Value::BulkStr(result))
    }
}

pub struct ArrParser;
impl Parser for ArrParser {
    const STARTING_BYTE: u8 = b'*';

    fn deserialize<R: io::BufRead>(reader: &mut R) -> Result<Value, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte)?;

        let arr_len = Self::deserialize_len(reader, &mut control_byte)?;
        let mut values = vec![];
        values.reserve(arr_len);

        for _ in 0..arr_len {
            let next_value = AnyParser::deserialize(reader)?;
            values.push(next_value);
        }
        Ok(Value::Arr(values))
    }
}

pub struct AnyParser;
impl Parser for AnyParser {
    const STARTING_BYTE: u8 = b'0';

    fn deserialize<R: io::BufRead>(reader: &mut R) -> Result<Value, RespError> {
        let next_byte = Self::peek_next_byte(reader)?;
        match next_byte {
            StrParser::STARTING_BYTE => StrParser::deserialize(reader),
            ErrParser::STARTING_BYTE => ErrParser::deserialize(reader),
            IntParser::STARTING_BYTE => IntParser::deserialize(reader),
            BulkStrParser::STARTING_BYTE => BulkStrParser::deserialize(reader),
            ArrParser::STARTING_BYTE => ArrParser::deserialize(reader),
            other => Err(RespError::UnknownStartingByte(other))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use io::Cursor;

    #[test]
    fn test_str_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"-wrong start\r\n");
            let result = StrParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b'+', actual: b'-' });
        }
        {   // correct empty string
            let mut cursor = Cursor::new(b"+\r\n");
            let result = StrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("".into()));
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
        {   // correct empty string
            let mut cursor = Cursor::new(b"-\r\n");
            let result = ErrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Err("".into()));
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
        {   // empty string (incorrect)
            let mut cursor = Cursor::new(b":\r\n");
            let result = IntParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::ParseInt(IntErrorKind::Empty));
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
            let mut cursor = Cursor::new(b"$8\r\nwrong len\r\n");
            let result = BulkStrParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::WrongBulkStrEnding(*b"n\r"));
        }
        {   // incorrect null value
            let mut cursor = Cursor::new(b"$-10\r\n");
            let result = BulkStrParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::WrongNullValue(*b"-10\r"));
        }
        {   // correct null value
            let mut cursor = Cursor::new(b"$-1\r\n");
            let result = BulkStrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Null);
        }
        {   // correct empty string
            let mut cursor = Cursor::new(b"$0\r\n\r\n");
            let result = BulkStrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::BulkStr("".into()));
        }
        {   // correct case (with CRLF in the middle)
            let mut cursor = Cursor::new(b"$15\r\ncorrect\r\ncase\n\r\r\n");
            let result = BulkStrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::BulkStr("correct\r\ncase\n\r".into()));
        }
    }

    #[test]
    fn test_arr_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"+3\r\n$4\r\nsome\r\n+number\r\n:321\r\n");
            let result = ArrParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b'*', actual: b'+' });
        }
        {   // correct empty array
            let mut cursor = Cursor::new(b"*0\r\n\r\n");
            let result = ArrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Arr(vec![]));
        }
        {   // correct case 1 [BulkStr, Str, Int]
            let mut cursor = Cursor::new(b"*3\r\n$4\r\nsome\r\n+number\r\n:321\r\n");
            let result = ArrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            let expected = Value::Arr(vec![
                Value::BulkStr("some".into()),
                Value::Str("number".into()),
                Value::Int(321),
            ]);
            assert_eq!(result.unwrap(), expected);
        }
        {   // correct case 2 [[Int, Int, Int], [Str, Str], [Err]]
            let mut cursor = Cursor::new(b"*3\r\n*3\r\n:1\r\n:2\r\n:3\r\n*2\r\n+Hello\r\n+World\r\n*1\r\n-some error\r\n");
            let result = ArrParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            let expected = Value::Arr(vec![
                Value::Arr(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
                Value::Arr(vec![Value::Str("Hello".into()), Value::Str("World".into())]),
                Value::Arr(vec![Value::Err("some error".into())]),
            ]);
            assert_eq!(result.unwrap(), expected);
        }
    }

    #[test]
    fn test_any_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"!\r\nwrong start\r\n");
            let result = AnyParser::deserialize(&mut cursor);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::UnknownStartingByte(b'!'));
        }
        {   // correct case (Str)
            let mut cursor = Cursor::new(b"+with \r in the end\r\r\n");
            let result = AnyParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("with \r in the end\r".into()));
        }
        {   // correct case (Err)
            let mut cursor = Cursor::new(b"-some\nerror\n\r\r\n");
            let result = AnyParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Err("some\nerror\n\r".into()));
        }
        {   // correct case (Int)
            let mut cursor = Cursor::new(b":12345678\r\n");
            let result = AnyParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Int(12345678));
        }
        {   // correct case (BulkStr)
            let mut cursor = Cursor::new(b"$14\r\ncorrect\ncase\n\r\r\n");
            let result = AnyParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::BulkStr("correct\ncase\n\r".into()));
        }
        {   // correct case (Arr)
            let mut cursor = Cursor::new(b"*3\r\n*3\r\n:1\r\n:2\r\n:3\r\n*2\r\n+Hello\r\n+World\r\n*1\r\n-some error\r\n");
            let result = AnyParser::deserialize(&mut cursor);
            assert!(result.is_ok());
            let expected = Value::Arr(vec![
                Value::Arr(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
                Value::Arr(vec![Value::Str("Hello".into()), Value::Str("World".into())]),
                Value::Arr(vec![Value::Err("some error".into())]),
            ]);
            assert_eq!(result.unwrap(), expected);
        }
    }

}
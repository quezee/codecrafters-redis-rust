use crate::resp_value::Value;

use std::marker::Unpin;
use std::{io, num::{ParseIntError, IntErrorKind}, string::FromUtf8Error};
use tokio::io::{AsyncReadExt, AsyncBufReadExt};


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

pub trait AsyncReader: AsyncBufReadExt + AsyncReadExt + Unpin {}
impl<T: AsyncBufReadExt + AsyncReadExt + Unpin> AsyncReader for T {} 

#[allow(async_fn_in_trait)]
pub trait Parser {
    const STARTING_BYTE: u8;

    async fn deserialize<R: AsyncReader>(reader: &mut R) -> Result<Value, RespError>;

    async fn check_starting_byte<R: AsyncReader>(reader: &mut R, control_byte: &mut [u8]) -> Result<(), RespError> {
        reader.read_exact(control_byte).await?;
        if control_byte[0] != Self::STARTING_BYTE {
            return Err(RespError::StartingByte{expected: Self::STARTING_BYTE, actual: control_byte[0]});
        }
        Ok(())
    }

    async fn peek_next_byte<R: AsyncReader>(reader: &mut R) -> io::Result<u8> {
        let buf = reader.fill_buf().await?;
        match buf.first() {
            Some(byte) => Ok(*byte),
            None => Err(io::Error::from(io::ErrorKind::UnexpectedEof))
        }
    }

    async fn read_until_crlf<R: AsyncReader>(reader: &mut R, control_byte: &mut [u8]) -> Result<Vec<u8>, RespError> {
        let mut buf = vec![];

        reader.read_until(b'\r', &mut buf).await?;
        reader.read_exact(control_byte).await?;

        while control_byte[0] != b'\n' {
            buf.extend_from_slice(control_byte);
            if control_byte[0] == b'\r' {
                reader.read_exact(control_byte).await?;
                continue;
            }
            reader.read_until(b'\r', &mut buf).await?;
            reader.read_exact(control_byte).await?;
        }
        buf.pop();
        Ok(buf)
    }

    async fn deserialize_str<R: AsyncReader>(reader: &mut R) -> Result<String, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte).await?;

        let buf = Self::read_until_crlf(reader, &mut control_byte).await?;
        let result = String::from_utf8(buf)?;
        Ok(result)
    }

    async fn deserialize_null<R: AsyncReader>(reader: &mut R) -> Result<(), RespError> {
        // `-1\r\n` expected
        let mut null_val_buf = [0u8; 4];
        reader.read_exact(&mut null_val_buf).await?;
        if &null_val_buf == b"-1\r\n" {
            return Ok(());
        } else {
            return Err(RespError::WrongNullValue(null_val_buf));
        }
    }

    async fn deserialize_len<R: AsyncReader>(reader: &mut R, control_byte: &mut [u8]) -> Result<usize, RespError> {
        let buf = Self::read_until_crlf(reader, control_byte).await?;
        let buf_str = String::from_utf8(buf)?;
        let len = buf_str.parse::<usize>()?;
        Ok(len)
    }
}

pub struct StrParser;
impl Parser for StrParser {
    const STARTING_BYTE: u8 = b'+';

    async fn deserialize<R: AsyncReader>(reader: &mut R) -> Result<Value, RespError> {
        let result = Self::deserialize_str(reader).await?;
        Ok(Value::Str(result))
    }
}

pub struct ErrParser;
impl Parser for ErrParser {
    const STARTING_BYTE: u8 = b'-';

    async fn deserialize<R: AsyncReader>(reader: &mut R) -> Result<Value, RespError> {
        let result = Self::deserialize_str(reader).await?;
        Ok(Value::Err(result))
    }
}

pub struct IntParser;
impl Parser for IntParser {
    const STARTING_BYTE: u8 = b':';

    async fn deserialize<R: AsyncReader>(reader: &mut R) -> Result<Value, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte).await?;

        let buf = Self::read_until_crlf(reader, &mut control_byte).await?;
        let buf_str = String::from_utf8(buf)?;
        let result = buf_str.parse::<i64>()?;
        Ok(Value::Int(result))
    }
}

pub struct BulkStrParser;
impl Parser for BulkStrParser {
    const STARTING_BYTE: u8 = b'$';

    async fn deserialize<R: AsyncReader>(reader: &mut R) -> Result<Value, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte).await?;

        let next_byte = Self::peek_next_byte(reader).await?;
        if next_byte == b'-' {
            Self::deserialize_null(reader).await?;
            return Ok(Value::BulkStr(None));
        }

        let str_len = Self::deserialize_len(reader, &mut control_byte).await?;
        let mut buf = vec![0u8; str_len];
        reader.read_exact(&mut buf).await?;

        let mut ending_bytes = [0u8; 2];
        reader.read_exact(&mut ending_bytes).await?;
        if &ending_bytes != b"\r\n" {
            return Err(RespError::WrongBulkStrEnding(ending_bytes));
        }

        Ok(Value::BulkStr(Some(buf)))
    }
}

pub struct ArrParser;
impl Parser for ArrParser {
    const STARTING_BYTE: u8 = b'*';

    async fn deserialize<R: AsyncReader>(reader: &mut R) -> Result<Value, RespError> {
        let mut control_byte = [0u8];
        Self::check_starting_byte(reader, &mut control_byte).await?;

        let next_byte = Self::peek_next_byte(reader).await?;
        if next_byte == b'-' {
            Self::deserialize_null(reader).await?;
            return Ok(Value::Arr(None));
        }

        let arr_len = Self::deserialize_len(reader, &mut control_byte).await?;
        let mut values = vec![];
        values.reserve(arr_len);

        for _ in 0..arr_len {
            let next_value = Box::pin(AnyParser::deserialize(reader)).await?;
            values.push(next_value);
        }
        Ok(Value::Arr(Some(values)))
    }
}

pub struct AnyParser;
impl Parser for AnyParser {
    const STARTING_BYTE: u8 = b'0';

    async fn deserialize<R: AsyncReader>(reader: &mut R) -> Result<Value, RespError> {
        let next_byte = Self::peek_next_byte(reader).await?;
        match next_byte {
            StrParser::STARTING_BYTE => StrParser::deserialize(reader).await,
            ErrParser::STARTING_BYTE => ErrParser::deserialize(reader).await,
            IntParser::STARTING_BYTE => IntParser::deserialize(reader).await,
            BulkStrParser::STARTING_BYTE => BulkStrParser::deserialize(reader).await,
            ArrParser::STARTING_BYTE => ArrParser::deserialize(reader).await,
            other => Err(RespError::UnknownStartingByte(other))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use io::Cursor;

    #[tokio::test]
    async fn test_str_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"-wrong start\r\n");
            let result = StrParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b'+', actual: b'-' });
        }
        {   // correct empty string
            let mut cursor = Cursor::new(b"+\r\n");
            let result = StrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("".into()));
        }
        {   // simple case
            let mut cursor = Cursor::new(b"+simple case\r\n");
            let result = StrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("simple case".into()));
        }
        {   // CR in the middle and in the end
            let mut cursor = Cursor::new(b"+with \r in the end\r\r\n");
            let result = StrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("with \r in the end\r".into()));
        }
        {   // LF in the middle and in the end
            let mut cursor = Cursor::new(b"+with \n in the end\n\r\n");
            let result = StrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("with \n in the end\n".into()));
        }
    }

    #[tokio::test]
    async fn test_err_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"+wrong start\r\n");
            let result = ErrParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b'-', actual: b'+' });
        }
        {   // correct empty string
            let mut cursor = Cursor::new(b"-\r\n");
            let result = ErrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Err("".into()));
        }
        {   // simple case
            let mut cursor = Cursor::new(b"-simple case\r\n");
            let result = ErrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Err("simple case".into()));
        }
    }

    #[tokio::test]
    async fn test_int_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"+123\r\n");
            let result = IntParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b':', actual: b'+' });
        }
        {   // non-numerical symbol
            let mut cursor = Cursor::new(b":1a3\r\n");
            let result = IntParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::ParseInt(IntErrorKind::InvalidDigit));
        }
        {   // empty string (incorrect)
            let mut cursor = Cursor::new(b":\r\n");
            let result = IntParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::ParseInt(IntErrorKind::Empty));
        }
        {   // correct int
            let mut cursor = Cursor::new(b":123\r\n");
            let result = IntParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Int(123));
        }
    }

    #[tokio::test]
    async fn test_bulk_str_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"+11\r\nwrong start\r\n");
            let result = BulkStrParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b'$', actual: b'+' });
        }
        {   // wrong declared len
            let mut cursor = Cursor::new(b"$8\r\nwrong len\r\n");
            let result = BulkStrParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::WrongBulkStrEnding(*b"n\r"));
        }
        {   // incorrect null value
            let mut cursor = Cursor::new(b"$-10\r\n");
            let result = BulkStrParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::WrongNullValue(*b"-10\r"));
        }
        {   // correct null value
            let mut cursor = Cursor::new(b"$-1\r\n");
            let result = BulkStrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::BulkStr(None));
        }
        {   // correct empty string
            let mut cursor = Cursor::new(b"$0\r\n\r\n");
            let result = BulkStrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::BulkStr(Some("".into())));
        }
        {   // correct case (with CRLF in the middle)
            let mut cursor = Cursor::new(b"$15\r\ncorrect\r\ncase\n\r\r\n");
            let result = BulkStrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::BulkStr(Some("correct\r\ncase\n\r".into())));
        }
    }

    #[tokio::test]
    async fn test_arr_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"+3\r\n$4\r\nsome\r\n+number\r\n:321\r\n");
            let result = ArrParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::StartingByte { expected: b'*', actual: b'+' });
        }
        {   // incorrect null array
            let mut cursor = Cursor::new(b"*-111\r\n");
            let result = ArrParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::WrongNullValue(*b"-111"));
        }
        {   // correct null array
            let mut cursor = Cursor::new(b"*-1\r\n");
            let result = ArrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Arr(None));
        }
        {   // correct empty array
            let mut cursor = Cursor::new(b"*0\r\n");
            let result = ArrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Arr(Some(vec![])));
        }
        {   // correct case 1 [BulkStr, Str, Int]
            let mut cursor = Cursor::new(b"*3\r\n$4\r\nsome\r\n+number\r\n:321\r\n");
            let result = ArrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            let expected = Value::Arr(Some(vec![
                Value::BulkStr(Some("some".into())),
                Value::Str("number".into()),
                Value::Int(321),
            ]));
            assert_eq!(result.unwrap(), expected);
        }
        {   // correct case 2 [[Int, Int, Int], [Str, Str], [Err]]
            let mut cursor = Cursor::new(b"*3\r\n*3\r\n:1\r\n:2\r\n:3\r\n*2\r\n+Hello\r\n+World\r\n*1\r\n-some error\r\n");
            let result = ArrParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            let expected = Value::Arr(Some(vec![
                Value::Arr(Some(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
                Value::Arr(Some(vec![Value::Str("Hello".into()), Value::Str("World".into())])),
                Value::Arr(Some(vec![Value::Err("some error".into())])),
            ]));
            assert_eq!(result.unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn test_any_parser() {
        {   // wrong starting byte
            let mut cursor = Cursor::new(b"!\r\nwrong start\r\n");
            let result = AnyParser::deserialize(&mut cursor).await;
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), RespError::UnknownStartingByte(b'!'));
        }
        {   // correct case (Str)
            let mut cursor = Cursor::new(b"+with \r in the end\r\r\n");
            let result = AnyParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Str("with \r in the end\r".into()));
        }
        {   // correct case (Err)
            let mut cursor = Cursor::new(b"-some\nerror\n\r\r\n");
            let result = AnyParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Err("some\nerror\n\r".into()));
        }
        {   // correct case (Int)
            let mut cursor = Cursor::new(b":12345678\r\n");
            let result = AnyParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::Int(12345678));
        }
        {   // correct case (BulkStr)
            let mut cursor = Cursor::new(b"$14\r\ncorrect\ncase\n\r\r\n");
            let result = AnyParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Value::BulkStr(Some("correct\ncase\n\r".into())));
        }
        {   // correct case (Arr)
            let mut cursor = Cursor::new(b"*3\r\n*3\r\n:1\r\n:2\r\n:3\r\n*2\r\n+Hello\r\n+World\r\n*1\r\n-some error\r\n");
            let result = AnyParser::deserialize(&mut cursor).await;
            assert!(result.is_ok());
            let expected = Value::Arr(Some(vec![
                Value::Arr(Some(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
                Value::Arr(Some(vec![Value::Str("Hello".into()), Value::Str("World".into())])),
                Value::Arr(Some(vec![Value::Err("some error".into())])),
            ]));
            assert_eq!(result.unwrap(), expected);
        }
    }

}
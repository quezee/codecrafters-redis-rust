#[derive(Debug, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Err(String),
    Int(i64),
    BulkStr(String),
    Arr(Vec<Value>),
}

// impl Value {
//     fn serialize<W: io::Write>(writer: &mut W) -> io::Result<()> {
//     }
// }

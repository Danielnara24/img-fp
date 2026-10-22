//! A record the run writes as columns: a report's row, a `--dump` verdict.
//!
//! **A column is said once**: its name, how its cell is read off the row,
//! and whether the CSV and the JSON carry it. The CSV's header, the CSV's
//! cells and the JSON's keys are all read off that one list, in its order.
//! Before, each layout wrote a column four times — a field of a row struct
//! with its serde attributes, a line where the row was filled in, an entry
//! in a header array whose length was counted by hand, and a cell in a list
//! that matched the header by position alone — so two cells swapped, or a
//! column added to the header and not to the rows, compiled and passed, and
//! shifted every CSV a script reads by a column.

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;
use std::borrow::Cow;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// One cell's value, as every layout writes it.
pub enum Cell<'a> {
    /// Not measured, or nothing to name: an empty cell, and in the JSON a
    /// `null` or no key at all (`Json`).
    None,
    Text(Cow<'a, str>),
    /// A file. The CSV writes its own bytes; the JSON its name as text and,
    /// when that is not UTF-8, its bytes beside it under `NAME_bytes`
    /// (`raw_bytes`).
    Path(&'a Path),
    Count(u64),
    /// A figure, written as the shortest decimal that reads back as itself.
    Figure(f32),
    /// A figure to so many decimals. The CSV's alone.
    Fixed(f32, usize),
    Yes(bool),
}

impl Cell<'_> {
    /// `true`, or nothing: a fact said only where it holds.
    pub fn flag(on: bool) -> Cell<'static> {
        if on {
            Cell::Yes(true)
        } else {
            Cell::None
        }
    }

    /// What `x` gives, or nothing.
    pub fn of<T>(x: Option<T>, f: impl FnOnce(T) -> Cell<'static>) -> Cell<'static> {
        x.map_or(Cell::None, f)
    }
}

/// What the JSON does with a column.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Json {
    /// Always a key: `null` when there is nothing.
    Always,
    /// A key only when there is something.
    Present,
    /// The CSV's alone.
    Never,
}

/// One column: its name, whether each layout carries it, and its cell.
pub struct Column<R> {
    pub name: &'static str,
    pub csv: bool,
    pub json: Json,
    pub cell: for<'r> fn(&'r R) -> Cell<'r>,
}

/// A column in both layouts, its JSON key always there.
pub const fn both<R>(name: &'static str, cell: for<'r> fn(&'r R) -> Cell<'r>) -> Column<R> {
    Column { name, csv: true, json: Json::Always, cell }
}

/// A list of columns, in the order every layout writes them.
pub struct Table<R>(pub Vec<Column<R>>);

impl<R> Table<R> {
    /// The CSV's header line.
    pub fn csv_header(&self, w: &mut dyn Write, sep: u8) -> std::io::Result<()> {
        let names: Vec<&[u8]> = self.0.iter().filter(|c| c.csv).map(|c| c.name.as_bytes()).collect();
        csv_row(w, &names, sep)
    }

    /// `row` as a CSV line: what was not measured is an empty cell.
    pub fn csv(&self, w: &mut dyn Write, row: &R, sep: u8) -> std::io::Result<()> {
        let cells: Vec<Cow<[u8]>> = self
            .0
            .iter()
            .filter(|c| c.csv)
            .map(|c| match (c.cell)(row) {
                Cell::None => Cow::Borrowed(b"".as_slice()),
                Cell::Text(Cow::Borrowed(s)) => Cow::Borrowed(s.as_bytes()),
                Cell::Text(Cow::Owned(s)) => Cow::Owned(s.into_bytes()),
                Cell::Path(p) => Cow::Borrowed(p.as_os_str().as_bytes()),
                Cell::Count(n) => Cow::Owned(n.to_string().into_bytes()),
                Cell::Figure(x) => Cow::Owned(x.to_string().into_bytes()),
                Cell::Fixed(x, d) => Cow::Owned(format!("{x:.d$}").into_bytes()),
                Cell::Yes(b) => Cow::Borrowed(if b { b"true".as_slice() } else { b"false".as_slice() }),
            })
            .collect();
        let cells: Vec<&[u8]> = cells.iter().map(|c| c.as_ref()).collect();
        csv_row(w, &cells, sep)
    }

    /// `row` as a JSON object.
    pub fn json<'t>(&'t self, row: &'t R) -> Object<'t, R> {
        Object { table: self, row }
    }

    /// `rows` as a JSON list of objects.
    pub fn rows<'t>(&'t self, rows: &'t [R]) -> Rows<'t, R> {
        Rows { table: self, rows }
    }
}

/// Rows as the JSON writes them: a list of `Object`s.
pub struct Rows<'t, R> {
    table: &'t Table<R>,
    rows: &'t [R],
}

impl<R> Serialize for Rows<'_, R> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.rows.iter().map(|r| self.table.json(r)))
    }
}

/// A row as the JSON writes it: one key a column, in the table's order.
pub struct Object<'t, R> {
    table: &'t Table<R>,
    row: &'t R,
}

impl<R> Serialize for Object<'_, R> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        for c in self.table.0.iter().filter(|c| c.json != Json::Never) {
            match (c.cell)(self.row) {
                Cell::None if c.json == Json::Present => {}
                Cell::None => m.serialize_entry(c.name, &())?,
                Cell::Text(t) => m.serialize_entry(c.name, &t)?,
                Cell::Path(p) => path_entry(&mut m, c.name, p)?,
                Cell::Count(n) => m.serialize_entry(c.name, &n)?,
                Cell::Figure(x) | Cell::Fixed(x, _) => m.serialize_entry(c.name, &x)?,
                Cell::Yes(b) => m.serialize_entry(c.name, &b)?,
            }
        }
        m.end()
    }
}

/// A file under `name` in a JSON object, as every path in a report is: its
/// name as text, and its bytes under `NAME_bytes` when that is not UTF-8.
pub fn path_entry<M: SerializeMap>(m: &mut M, name: &str, p: &Path) -> Result<(), M::Error> {
    m.serialize_entry(name, &p.to_string_lossy())?;
    if let Some(b) = raw_bytes(p) {
        m.serialize_entry(&format!("{name}_bytes"), &b)?;
    }
    Ok(())
}

/// A path's bytes, when they are not UTF-8 and so cannot be a JSON string.
///
/// Every path in the JSON is written as text, and a name that is not UTF-8
/// comes out of that with `U+FFFD` where its odd bytes were: readable, and the
/// name of no file on disk. Anything that acts on the report — a script
/// deleting the matches, or the window moving one to the Trash — needs the
/// name itself, so such a path also carries its bytes beside it, as an array
/// of numbers. A UTF-8 path, which is nearly every path, carries nothing.
pub fn raw_bytes(p: &Path) -> Option<Vec<u8>> {
    match p.to_str() {
        Some(_) => None,
        None => Some(p.as_os_str().as_bytes().to_vec()),
    }
}

/// One CSV line with `sep` between the fields: a field holding the
/// separator, a quote or a line break is quoted, its quotes doubled — RFC
/// 4180's rule, which is what every spreadsheet reads — and every field is
/// written as its own bytes. Only a path can need quoting.
fn csv_row(w: &mut dyn Write, fields: &[&[u8]], sep: u8) -> std::io::Result<()> {
    for (k, f) in fields.iter().enumerate() {
        if k > 0 {
            w.write_all(&[sep])?;
        }
        if f.iter().any(|&b| b == sep || matches!(b, b'"' | b'\n' | b'\r')) {
            w.write_all(b"\"")?;
            for piece in f.split_inclusive(|&b| b == b'"') {
                w.write_all(piece)?;
                if piece.ends_with(b"\"") {
                    w.write_all(b"\"")?;
                }
            }
            w.write_all(b"\"")?;
        } else {
            w.write_all(f)?;
        }
    }
    w.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct R {
        path: PathBuf,
        n: Option<u32>,
        x: f32,
        odd: bool,
    }

    fn table() -> Table<R> {
        Table(vec![
            Column { name: "only_csv", csv: true, json: Json::Never, cell: |_| Cell::Text("c".into()) },
            both("path", |r| Cell::Path(&r.path)),
            both("n", |r| Cell::of(r.n, |n| Cell::Count(n.into()))),
            both("x", |r| Cell::Figure(r.x)),
            Column { name: "odd", csv: true, json: Json::Present, cell: |r| Cell::flag(r.odd) },
            Column { name: "only_json", csv: false, json: Json::Present, cell: |r| Cell::of(r.n, |_| Cell::Yes(false)) },
        ])
    }

    fn written(r: &R) -> (Vec<u8>, String) {
        let t = table();
        let mut csv = Vec::new();
        t.csv_header(&mut csv, b';').unwrap();
        t.csv(&mut csv, r, b';').unwrap();
        (csv, serde_json::to_string(&t.json(r)).unwrap())
    }

    #[test]
    fn every_layout_reads_one_list_of_columns() {
        let (csv, json) = written(&R { path: "/a;b.jpg".into(), n: Some(7), x: 0.1, odd: true });
        assert_eq!(csv, b"only_csv;path;n;x;odd\nc;\"/a;b.jpg\";7;0.1;true\n");
        assert_eq!(json, r#"{"path":"/a;b.jpg","n":7,"x":0.1,"odd":true,"only_json":false}"#);
        let (csv, json) = written(&R { path: "/a.jpg".into(), n: None, x: 1.0, odd: false });
        assert_eq!(csv, b"only_csv;path;n;x;odd\nc;/a.jpg;;1;\n");
        assert_eq!(json, r#"{"path":"/a.jpg","n":null,"x":1.0}"#, "nothing is a null, or no key at all");
    }

    #[test]
    fn a_quote_in_a_csv_field_is_doubled() {
        let mut buf = Vec::new();
        csv_row(&mut buf, &[b"a\"b;c", b"plain"], b';').unwrap();
        assert_eq!(buf, b"\"a\"\"b;c\";plain\n");
    }

    #[test]
    fn a_path_that_is_not_utf8_carries_its_bytes_beside_it() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(b"/\xff\"q".to_vec()));
        let (csv, json) = written(&R { path, n: None, x: 0.5, odd: false });
        assert!(csv.ends_with(b"\nc;\"/\xff\"\"q\";;0.5;\n"));
        assert_eq!(json, r#"{"path":"/�\"q","path_bytes":[47,255,34,113],"n":null,"x":0.5}"#);
    }
}

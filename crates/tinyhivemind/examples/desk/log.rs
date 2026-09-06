//! A JSONL-backed [`SessionLog`], which is the host's side of the boundary.
//!
//! The library never opens a file; this example does, because a shared
//! transcript several agents read and write has to live somewhere. Rows are
//! appended one JSON object per line and the whole file is held in memory,
//! which is honest for a desk that runs for an hour and wrong for a company
//! that runs for a year — [`SessionLog::read_before`] documents the tail-read
//! a real host owes.

use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tinyhivemind::{LogMessage, SessionAuthor, SessionFuture, SessionLog, SessionPage, Sequence};

/// An append-only transcript on disk, readable newest-first.
pub struct JsonlLog {
    path: PathBuf,
    rows: Mutex<Vec<LogMessage>>,
}

impl JsonlLog {
    /// Open, creating the file if it does not exist, and load what is there.
    ///
    /// # Errors
    ///
    /// Returns any filesystem or decode failure.
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut rows = Vec::new();
        if path.exists() {
            for line in BufReader::new(File::open(path)?).lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                rows.push(serde_json::from_str::<LogMessage>(&line)?);
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            rows: Mutex::new(rows),
        })
    }

    /// Append one row and return the sequence it was given.
    ///
    /// # Errors
    ///
    /// Returns any filesystem or encode failure.
    pub fn append(
        &self,
        chat_id: Option<String>,
        author: SessionAuthor,
        content: &str,
    ) -> Result<Sequence, Box<dyn std::error::Error + Send + Sync>> {
        let mut rows = self.rows.lock().expect("transcript lock is never poisoned");
        let sequence = Sequence(rows.len() as u64 + 1);
        let row = LogMessage {
            sequence,
            chat_id,
            parent: None,
            author,
            content: content.to_string(),
        };
        let mut file = OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(file, "{}", serde_json::to_string(&row)?)?;
        rows.push(row);
        Ok(sequence)
    }

    /// How many rows the transcript holds.
    pub fn len(&self) -> usize {
        self.rows.lock().expect("transcript lock").len()
    }
}

impl SessionLog for JsonlLog {
    fn read_before(&self, before: Option<Sequence>, limit: usize) -> SessionFuture<'_> {
        let rows = self.rows.lock().expect("transcript lock");
        let ceiling = before.map_or(u64::MAX, |sequence| sequence.0);
        let mut messages: Vec<LogMessage> = rows
            .iter()
            .rev()
            .filter(|row| row.sequence.0 < ceiling)
            .take(limit)
            .cloned()
            .collect();
        messages.sort_by(|left, right| right.sequence.cmp(&left.sequence));
        let next_before = messages.last().map(|row| row.sequence);
        let page = SessionPage {
            messages,
            next_before,
        };
        Box::pin(async move { Ok(page) })
    }
}

use crate::envelope::BoardPost;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use std::sync::{Mutex, MutexGuard};

pub struct Store(Mutex<Connection>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestOutcome {
    Stored { is_op: bool },
    Duplicate,
    MissingParent,
    Invalid,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CatalogRow {
    pub op_txid: String,
    pub board: String,
    pub subject: String,
    pub body: String,
    pub ephemeral_pk: String,
    pub recovery_nonce: Option<String>,
    pub image_sha256: Option<String>,
    pub created_daa: i64,
    pub bump_daa: i64,
    pub post_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PostRow {
    pub txid: String,
    pub board: String,
    pub thread_op_txid: String,
    pub subject: String,
    pub body: String,
    pub ephemeral_pk: String,
    pub recovery_nonce: Option<String>,
    pub image_sha256: Option<String>,
    pub daa: i64,
    pub index: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ThreadView {
    pub op_txid: String,
    pub board: String,
    pub subject: String,
    pub bump_daa: i64,
    pub post_count: i64,
    pub posts: Vec<PostRow>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StatusRow {
    pub posts: i64,
    pub threads: i64,
    pub pending_replies: i64,
    pub last_hash: Option<String>,
    pub last_daa: Option<String>,
}

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        let connection =
            Connection::open(path).with_context(|| format!("open SQLite at {path}"))?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS threads(
                 op_txid TEXT PRIMARY KEY,
                 board TEXT NOT NULL,
                 subject TEXT NOT NULL,
                 created_daa INTEGER NOT NULL,
                 bump_daa INTEGER NOT NULL,
                 post_count INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS posts(
                 txid TEXT PRIMARY KEY,
                 thread_op_txid TEXT NOT NULL REFERENCES threads(op_txid),
                 board TEXT NOT NULL,
                 subject TEXT NOT NULL,
                 body TEXT NOT NULL,
                 ephemeral_pk TEXT NOT NULL,
                 recovery_nonce TEXT,
                 image_sha256 TEXT,
                 daa INTEGER NOT NULL,
                 idx INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS seen_envelopes(
                 envelope_sha256 TEXT PRIMARY KEY,
                 txid TEXT NOT NULL,
                 seen_at_daa INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS pending_replies(
                 txid TEXT PRIMARY KEY,
                 parent_txid TEXT NOT NULL,
                 daa INTEGER NOT NULL,
                 payload BLOB NOT NULL,
                 seen_at_daa INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS scan_state(
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS threads_catalog ON threads(board, bump_daa DESC, op_txid);
             CREATE INDEX IF NOT EXISTS posts_thread ON posts(thread_op_txid, idx);
             CREATE INDEX IF NOT EXISTS pending_parent ON pending_replies(parent_txid, daa, txid);",
        )?;
        Ok(Self(Mutex::new(connection)))
    }

    fn connection(&self) -> MutexGuard<'_, Connection> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn note_envelope(&self, hash: &str, txid: &str, daa: i64) -> Result<bool> {
        let connection = self.connection();
        connection.execute(
            "INSERT OR IGNORE INTO seen_envelopes(envelope_sha256,txid,seen_at_daa)
             VALUES(?1,?2,?3)",
            params![hash, txid, daa],
        )?;
        let owner: String = connection.query_row(
            "SELECT txid FROM seen_envelopes WHERE envelope_sha256=?1",
            params![hash],
            |row| row.get(0),
        )?;
        Ok(owner == txid)
    }

    pub fn ingest(
        &self,
        txid: &str,
        daa: i64,
        post: &BoardPost,
        bump_limit: i64,
    ) -> Result<IngestOutcome> {
        let mut connection = self.connection();
        if let Some((thread, current_daa)) = connection
            .query_row(
                "SELECT thread_op_txid,daa FROM posts WHERE txid=?1",
                params![txid],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
        {
            if daa < current_daa {
                let transaction = connection.transaction()?;
                transaction.execute("UPDATE posts SET daa=?2 WHERE txid=?1", params![txid, daa])?;
                Self::renumber_thread(&transaction, &thread, bump_limit)?;
                transaction.commit()?;
            }
            return Ok(IngestOutcome::Duplicate);
        }

        let public_key = hex::encode(post.ephemeral_pk);
        let recovery_nonce = post.recovery_nonce.map(hex::encode);
        let image_sha256 = post.image_sha256.map(hex::encode);
        if post.is_op {
            let transaction = connection.transaction()?;
            transaction.execute(
                "INSERT INTO threads(op_txid,board,subject,created_daa,bump_daa,post_count)
                 VALUES(?1,?2,?3,?4,?4,1)",
                params![txid, post.board, post.subject, daa],
            )?;
            transaction.execute(
                "INSERT INTO posts(txid,thread_op_txid,board,subject,body,ephemeral_pk,
                                   recovery_nonce,image_sha256,daa,idx)
                 VALUES(?1,?1,?2,?3,?4,?5,?6,?7,?8,0)",
                params![
                    txid,
                    post.board,
                    post.subject,
                    post.body,
                    public_key,
                    recovery_nonce,
                    image_sha256,
                    daa
                ],
            )?;
            transaction.commit()?;
            return Ok(IngestOutcome::Stored { is_op: true });
        }

        let Some(parent) = post.parent_txid.map(hex::encode) else {
            return Ok(IngestOutcome::Invalid);
        };
        let Some(parent_board) = connection
            .query_row(
                "SELECT board FROM threads WHERE op_txid=?1",
                params![parent],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        else {
            return Ok(IngestOutcome::MissingParent);
        };
        if parent_board != post.board || !post.subject.is_empty() {
            return Ok(IngestOutcome::Invalid);
        }

        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO posts(txid,thread_op_txid,board,subject,body,ephemeral_pk,
                               recovery_nonce,image_sha256,daa,idx)
             VALUES(?1,?2,?3,'',?4,?5,?6,?7,?8,0)",
            params![
                txid,
                parent,
                post.board,
                post.body,
                public_key,
                recovery_nonce,
                image_sha256,
                daa
            ],
        )?;
        Self::renumber_thread(&transaction, &parent, bump_limit)?;
        transaction.commit()?;
        Ok(IngestOutcome::Stored { is_op: false })
    }

    fn renumber_thread(
        transaction: &Transaction<'_>,
        op_txid: &str,
        bump_limit: i64,
    ) -> Result<()> {
        let mut replies = Vec::new();
        {
            let mut statement = transaction.prepare(
                "SELECT txid FROM posts WHERE thread_op_txid=?1 AND txid<>?1 ORDER BY daa,txid",
            )?;
            for row in statement.query_map(params![op_txid], |row| row.get::<_, String>(0))? {
                replies.push(row?);
            }
        }
        transaction.execute("UPDATE posts SET idx=0 WHERE txid=?1", params![op_txid])?;
        for (position, txid) in replies.iter().enumerate() {
            transaction.execute(
                "UPDATE posts SET idx=?2 WHERE txid=?1",
                params![txid, position as i64 + 1],
            )?;
        }
        let count = replies.len() as i64 + 1;
        let bump_index = count.min(bump_limit.max(1)) - 1;
        let bump_daa: i64 = transaction.query_row(
            "SELECT daa FROM posts WHERE thread_op_txid=?1 AND idx=?2",
            params![op_txid, bump_index],
            |row| row.get(0),
        )?;
        let created_daa: i64 = transaction.query_row(
            "SELECT daa FROM posts WHERE txid=?1",
            params![op_txid],
            |row| row.get(0),
        )?;
        transaction.execute(
            "UPDATE threads SET post_count=?2,bump_daa=?3,created_daa=?4 WHERE op_txid=?1",
            params![op_txid, count, bump_daa, created_daa],
        )?;
        Ok(())
    }

    pub fn park_pending(
        &self,
        txid: &str,
        parent: &str,
        daa: i64,
        payload: &[u8],
        seen_at_daa: i64,
        max_pending: usize,
    ) -> Result<()> {
        let connection = self.connection();
        connection.execute(
            "INSERT OR IGNORE INTO pending_replies(txid,parent_txid,daa,payload,seen_at_daa)
             VALUES(?1,?2,?3,?4,?5)",
            params![txid, parent, daa, payload, seen_at_daa],
        )?;
        connection.execute(
            "DELETE FROM pending_replies WHERE txid IN (
                 SELECT txid FROM pending_replies
                 ORDER BY seen_at_daa DESC,txid DESC LIMIT -1 OFFSET ?1
             )",
            params![max_pending.max(1) as i64],
        )?;
        Ok(())
    }

    pub fn take_pending(&self, parent: &str) -> Result<Vec<(String, i64, Vec<u8>)>> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let mut pending = Vec::new();
        {
            let mut statement = transaction.prepare(
                "SELECT txid,daa,payload FROM pending_replies
                 WHERE parent_txid=?1 ORDER BY daa,txid",
            )?;
            for row in statement.query_map(params![parent], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })? {
                pending.push(row?);
            }
        }
        transaction.execute(
            "DELETE FROM pending_replies WHERE parent_txid=?1",
            params![parent],
        )?;
        transaction.commit()?;
        Ok(pending)
    }

    pub fn prune_pending(&self, min_seen_at_daa: i64) -> Result<usize> {
        Ok(self.connection().execute(
            "DELETE FROM pending_replies WHERE seen_at_daa<?1",
            params![min_seen_at_daa],
        )?)
    }

    pub fn state_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .connection()
            .query_row(
                "SELECT value FROM scan_state WHERE key=?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn state_set(&self, key: &str, value: &str) -> Result<()> {
        self.connection().execute(
            "INSERT INTO scan_state(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn boards(&self) -> Result<Vec<String>> {
        let connection = self.connection();
        let mut statement =
            connection.prepare("SELECT DISTINCT board FROM threads ORDER BY board")?;
        let boards = statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(boards)
    }

    pub fn catalog(
        &self,
        board: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<CatalogRow>> {
        let connection = self.connection();
        let sql = "SELECT t.op_txid,t.board,t.subject,p.body,p.ephemeral_pk,p.recovery_nonce,
                          p.image_sha256,t.created_daa,t.bump_daa,t.post_count
                   FROM threads t JOIN posts p ON p.txid=t.op_txid
                   WHERE (?1 IS NULL OR t.board=?1)
                   ORDER BY t.bump_daa DESC,t.op_txid DESC LIMIT ?2 OFFSET ?3";
        let mut statement = connection.prepare(sql)?;
        let rows = statement
            .query_map(
                params![
                    board,
                    limit.min(1000) as i64,
                    offset.min(i64::MAX as usize) as i64
                ],
                |row| {
                    Ok(CatalogRow {
                        op_txid: row.get(0)?,
                        board: row.get(1)?,
                        subject: row.get(2)?,
                        body: row.get(3)?,
                        ephemeral_pk: row.get(4)?,
                        recovery_nonce: row.get(5)?,
                        image_sha256: row.get(6)?,
                        created_daa: row.get(7)?,
                        bump_daa: row.get(8)?,
                        post_count: row.get(9)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn thread(&self, op_txid: &str, limit: usize, offset: usize) -> Result<Option<ThreadView>> {
        let connection = self.connection();
        let Some((board, subject, bump_daa, post_count)) = connection
            .query_row(
                "SELECT board,subject,bump_daa,post_count FROM threads WHERE op_txid=?1",
                params![op_txid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
        else {
            return Ok(None);
        };
        let mut statement = connection.prepare(
            "SELECT txid,board,thread_op_txid,subject,body,ephemeral_pk,recovery_nonce,
                    image_sha256,daa,idx
             FROM posts WHERE thread_op_txid=?1 ORDER BY idx LIMIT ?2 OFFSET ?3",
        )?;
        let posts = statement
            .query_map(
                params![
                    op_txid,
                    limit.min(10_000) as i64,
                    offset.min(i64::MAX as usize) as i64
                ],
                |row| {
                    Ok(PostRow {
                        txid: row.get(0)?,
                        board: row.get(1)?,
                        thread_op_txid: row.get(2)?,
                        subject: row.get(3)?,
                        body: row.get(4)?,
                        ephemeral_pk: row.get(5)?,
                        recovery_nonce: row.get(6)?,
                        image_sha256: row.get(7)?,
                        daa: row.get(8)?,
                        index: row.get(9)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(ThreadView {
            op_txid: op_txid.to_owned(),
            board,
            subject,
            bump_daa,
            post_count,
            posts,
        }))
    }

    pub fn status(&self) -> Result<StatusRow> {
        let connection = self.connection();
        let count = |table: &str| -> Result<i64> {
            Ok(
                connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?,
            )
        };
        Ok(StatusRow {
            posts: count("posts")?,
            threads: count("threads")?,
            pending_replies: count("pending_replies")?,
            last_hash: connection
                .query_row(
                    "SELECT value FROM scan_state WHERE key='last_hash'",
                    [],
                    |row| row.get(0),
                )
                .optional()?,
            last_daa: connection
                .query_row(
                    "SELECT value FROM scan_state WHERE key='last_daa'",
                    [],
                    |row| row.get(0),
                )
                .optional()?,
        })
    }
}

/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

//! Contains basic definitions for the sql crate and for any crate that wish
//! to implement traits to be used with the sql's queries macro.

#![deny(warnings, missing_docs, clippy::all, rustdoc::broken_intra_doc_links)]

pub mod error;
pub mod mysql;
pub mod sqlite;
pub mod transaction;

use anyhow::{bail, format_err, Context, Error};
use std::fmt::{self, Debug};
use std::sync::Arc;

// Used in docs
#[cfg(test)]
mod _unused {
    use sql as _;
    use sql_tests_lib as _;
}

/// Struct to store a set of write, read and read-only connections for a shard.
#[derive(Clone)]
pub struct SqlConnections {
    /// Write connection to the master
    pub write_connection: Connection,
    /// Read connection
    pub read_connection: Connection,
    /// Read master connection
    pub read_master_connection: Connection,
}

impl SqlConnections {
    /// Create SqlConnections from a single connection.
    pub fn new_single(connection: Connection) -> Self {
        Self {
            write_connection: connection.clone(),
            read_connection: connection.clone(),
            read_master_connection: connection,
        }
    }
}

/// Struct to store a set of write, read and read-only connections for a shard.
/// Plus a schema connection so that sqlite tables can be setup.
#[derive(Clone)]
pub struct SqlConnectionsWithSchema {
    /// Normal connections not used for schema creation
    connections: SqlConnections,
    /// Connections for schema creation, only populated for sqlite.
    /// This is separate from write_connection as test cases still rely on schema being
    /// present but empty when in readonly mode.
    schema_connection: Option<Connection>,
}

impl SqlConnectionsWithSchema {
    /// Create a new SqlConnectionsWithSchema
    pub fn new(connections: SqlConnections, schema_connection: Option<Connection>) -> Self {
        Self {
            connections,
            schema_connection,
        }
    }

    /// Create SqlConnections from a single connection.
    pub fn new_single(connection: Connection) -> Self {
        Self {
            connections: SqlConnections::new_single(connection.clone()),
            schema_connection: Some(connection),
        }
    }

    /// Get a reference to the regular connections
    pub fn connections(&self) -> &SqlConnections {
        &self.connections
    }

    /// Execute sql on the schema connection to create schema if not present
    /// For mysql the schema connection should be None as schema is setup in advance2
    pub fn create_schema(&self, schema_sql: &str) -> Result<(), Error> {
        match &self.schema_connection {
            Some(Connection::Sqlite(conn)) => conn
                .get_sqlite_guard()
                .execute_batch(schema_sql)
                .with_context(|| format_err!("failed sql: {}", schema_sql)),
            Some(_) => bail!("not expecting schema connection for mysql"),
            None => Ok(()),
        }
    }
}

impl From<SqlConnectionsWithSchema> for SqlConnections {
    fn from(from: SqlConnectionsWithSchema) -> SqlConnections {
        from.connections
    }
}

/// Struct to store a set of write, read and read-only connections for multiple shards.
#[derive(Clone)]
pub struct SqlShardedConnections {
    /// Write connections to the master for each shard
    pub write_connections: Vec<Connection>,
    /// Read connections for each shard
    pub read_connections: Vec<Connection>,
    /// Read master connections for each shard
    pub read_master_connections: Vec<Connection>,
}

impl SqlShardedConnections {
    /// Check if the struct is empty.
    pub fn is_empty(&self) -> bool {
        self.write_connections.is_empty()
    }
}

impl From<Vec<SqlConnections>> for SqlShardedConnections {
    fn from(shard_connections: Vec<SqlConnections>) -> Self {
        let mut write_connections = Vec::with_capacity(shard_connections.len());
        let mut read_connections = Vec::with_capacity(shard_connections.len());
        let mut read_master_connections = Vec::with_capacity(shard_connections.len());
        for connections in shard_connections.into_iter() {
            write_connections.push(connections.write_connection);
            read_connections.push(connections.read_connection);
            read_master_connections.push(connections.read_master_connection);
        }

        Self {
            read_connections,
            read_master_connections,
            write_connections,
        }
    }
}

/// Enum that generalizes over connections to Sqlite and MyRouter.
#[derive(Clone)]
pub enum Connection {
    /// Sqlite lets you use this crate with rusqlite connections such as in memory or on disk Sqlite
    /// databases, both useful in case of testing or local sql db use cases.
    Sqlite(Arc<sqlite::SqliteMultithreaded>),
    /// A variant used for the new Mysql client connection factory.
    Mysql(mysql::Connection),
}

impl From<sqlite::SqliteMultithreaded> for Connection {
    fn from(con: sqlite::SqliteMultithreaded) -> Self {
        Connection::Sqlite(Arc::new(con))
    }
}

impl From<mysql::Connection> for Connection {
    fn from(conn: mysql::Connection) -> Self {
        Connection::Mysql(conn)
    }
}

impl Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Connection::Sqlite(..) => write!(f, "Sqlite"),
            Connection::Mysql(..) => write!(f, "Mysql client"),
        }
    }
}

/// Value returned from a `write` type of query
pub struct WriteResult {
    last_insert_id: Option<u64>,
    affected_rows: u64,
}

impl WriteResult {
    /// Method made public for access from inside macros, you probably don't want to use it.
    pub fn new(last_insert_id: Option<u64>, affected_rows: u64) -> Self {
        WriteResult {
            last_insert_id,
            affected_rows,
        }
    }

    /// Return the id of last inserted row if any.
    pub fn last_insert_id(&self) -> Option<u64> {
        self.last_insert_id
    }

    /// Return number of rows affected by the `write` query
    pub fn affected_rows(&self) -> u64 {
        self.affected_rows
    }
}

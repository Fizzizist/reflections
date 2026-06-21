use turso::connection::Connection;

pub struct TodoService {
    conn: Connection,
}

impl TodoService {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

use std::{cell::LazyCell, time::Instant};

use floem::imbl::{HashSet, Vector};
use floem::prelude::*;
use rusqlite::{named_params, Connection};

use crate::todo::{Todo, TodoState};

thread_local! {
    pub static TODOS_STATE: LazyCell<TodosState> = LazyCell::new(|| {
        TodosState::new()
    });
}

#[derive(Debug, Clone, Copy)]
pub struct ActiveTodo {
    pub time_set: Instant,
    pub active: Option<TodoState>,
}
impl ActiveTodo {
    pub fn set(&mut self, todo: Option<TodoState>) {
        self.time_set = Instant::now();
        self.active = todo;
    }

    fn new() -> Self {
        Self {
            time_set: Instant::now(),
            active: None,
        }
    }
}

pub struct TodosState {
    db: Connection,
    pub todos: RwSignal<Vector<crate::todo::TodoState>>,
    pub selected: RwSignal<HashSet<TodoState>>,
    pub active: RwSignal<ActiveTodo>,
    pub time_stated: Instant,
}
impl TodosState {
    fn new() -> Self {
        let conn = Connection::open("todos.db").unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS todo (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                done INTEGER NOT NULL,
                description TEXT NOT NULL
            )",
            [],
        )
        .unwrap();
        let mut stmt = conn
            .prepare("SELECT id, done, description FROM todo")
            .unwrap();
        let todos = RwSignal::new(
            stmt.query_map([], |row| {
                Ok(Todo::new_from_db(
                    row.get(0)?,
                    row.get::<_, i64>(1)? != 0,
                    row.get::<_, String>(2)?,
                )
                .into())
            })
            .unwrap()
            .collect::<rusqlite::Result<Vector<TodoState>>>()
            .unwrap(),
        );
        drop(stmt);

        let selected = RwSignal::new(HashSet::new());
        let active = RwSignal::new(ActiveTodo::new());

        Self {
            db: conn,
            todos,
            selected,
            active,
            time_stated: Instant::now(),
        }
    }

    pub fn new_todo(&self) {
        self.todos
            .update(|todos| todos.push_back(Todo::new(false, "").into()));
    }

    pub fn refresh_db(&self) {
        let mut stmt = self
            .db
            .prepare("SELECT id, done, description FROM todo")
            .unwrap();
        let todos = stmt
            .query_map([], |row| {
                Ok(Todo::new_from_db(
                    row.get(0)?,
                    row.get::<_, i64>(1)? != 0,
                    row.get::<_, String>(2)?,
                )
                .into())
            })
            .unwrap()
            .collect::<rusqlite::Result<Vector<TodoState>>>()
            .unwrap();
        self.todos.update(|old_todos| {
            old_todos.retain(|todo| todo.db_id.get_untracked().is_some());

            // Update existing todos and track which new todos were matched
            let mut matched_new_todos = Vec::new();
            for (i, new_todo) in todos.iter().enumerate() {
                if let Some(new_db_id) = new_todo.db_id.get_untracked() {
                    if let Some(old_todo) = old_todos.iter().find(|t| t.db_id == Some(new_db_id)) {
                        old_todo.done.set(new_todo.done.get_untracked());
                        old_todo
                            .description
                            .set(new_todo.description.get_untracked());
                        matched_new_todos.push(i);
                    }
                }
            }
            // Add any new todos that weren't matched
            for (i, new_todo) in todos.iter().enumerate() {
                if !matched_new_todos.contains(&i) {
                    old_todos.push_back(*new_todo);
                }
            }
        });
    }

    pub fn create(&self, description: &str, done: bool) -> i64 {
        self.db
            .execute(
                "INSERT INTO todo (done, description) VALUES (:done, :description)",
                named_params! {
                    ":done": done as i32,
                    ":description": description,
                },
            )
            .unwrap();
        self.db.last_insert_rowid()
    }

    pub fn update_done(&self, id: i64, done: bool) {
        self.db
            .execute("UPDATE todo SET done = ?1 WHERE id = ?2", [done as i64, id])
            .unwrap();
    }

    pub fn update_description(&self, id: i64, description: &str) {
        self.db
            .execute(
                "UPDATE todo SET description = :desc WHERE id = :id",
                named_params! {":desc": description, ":id": id},
            )
            .unwrap();
    }

    pub fn delete(&self, db_id: Option<i64>, view_id: u64) {
        if let Some(db_id) = db_id {
            self.db
                .execute("DELETE FROM todo WHERE id = ?1", [db_id])
                .unwrap();
        }
        self.todos
            .update(|todos| todos.retain(|v| v.unique_id != view_id));
    }
}

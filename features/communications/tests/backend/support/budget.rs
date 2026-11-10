//! A bound on the work a read does, counted in SQLite's instructions rather than in time.
/// A generous instruction budget for one bounded read, independent of the clock: its work must
/// follow what was asked for rather than a conversation's message history.
pub struct ReadBudget<'a> {
    connection: &'a rusqlite::Connection,
    remaining: Box<std::cell::Cell<usize>>,
}
impl<'a> ReadBudget<'a> {
    pub fn new(connection: &'a rusqlite::Connection) -> Self {
        let budget = Self {
            connection,
            remaining: Box::new(std::cell::Cell::new(200)),
        };
        unsafe extern "C" fn step(context: *mut std::ffi::c_void) -> std::ffi::c_int {
            // SAFETY: the boxed counter stays at this address until Drop removes the callback.
            let remaining = unsafe { &*context.cast::<std::cell::Cell<usize>>() };
            remaining.set(remaining.get().saturating_sub(1));
            std::ffi::c_int::from(remaining.get() == 0)
        }
        // SAFETY: this test uses the connection only on this thread. The counter is boxed and
        // Drop removes its callback before releasing it or the connection.
        unsafe {
            rusqlite::ffi::sqlite3_progress_handler(
                connection.handle(),
                100,
                Some(step),
                (&*budget.remaining as *const std::cell::Cell<usize>)
                    .cast_mut()
                    .cast(),
            );
        }
        budget
    }
}
impl Drop for ReadBudget<'_> {
    fn drop(&mut self) {
        // SAFETY: the borrowed connection outlives this guard and remains on this thread.
        unsafe {
            rusqlite::ffi::sqlite3_progress_handler(
                self.connection.handle(),
                0,
                None,
                std::ptr::null_mut(),
            );
        }
    }
}

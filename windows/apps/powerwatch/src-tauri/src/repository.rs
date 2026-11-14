//! SQLite and the validated in-memory session share one lock and commit boundary.
use super::CommandError;
use powerwatch_core::session::{AddressBook, SessionError, SessionState};
use powerwatch_storage::Store;
pub(crate) struct Repository {
    pub store: Store,
    pub book: AddressBook,
}
impl Repository {
    pub fn update(
        &mut self,
        action: impl FnOnce(&mut AddressBook) -> Result<SessionState, SessionError>,
        purge: Option<&str>,
    ) -> Result<SessionState, CommandError> {
        let mut next = self.book.clone();
        let state = action(&mut next)?;
        self.store.save_book_with_purge(&next, purge)?;
        self.book = next;
        Ok(state)
    }
}

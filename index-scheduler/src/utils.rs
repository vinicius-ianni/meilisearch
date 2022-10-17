//! Utility functions on the DBs. Mainly getter and setters.

use meilisearch_types::heed::{types::DecodeIgnore, RoTxn, RwTxn};
use meilisearch_types::milli::BEU32;
use roaring::RoaringBitmap;

use crate::{Error, IndexScheduler, Result, Task, TaskId};
use meilisearch_types::tasks::{Kind, Status};

impl IndexScheduler {
    pub(crate) fn all_task_ids(&self, rtxn: &RoTxn) -> Result<RoaringBitmap> {
        let mut all_tasks = RoaringBitmap::new();
        for status in [
            Status::Enqueued,
            Status::Processing,
            Status::Succeeded,
            Status::Failed,
        ] {
            all_tasks |= self.get_status(&rtxn, status)?;
        }
        Ok(all_tasks)
    }
    pub(crate) fn last_task_id(&self, rtxn: &RoTxn) -> Result<Option<TaskId>> {
        Ok(self
            .all_tasks
            .remap_data_type::<DecodeIgnore>()
            .last(rtxn)?
            .map(|(k, _)| k.get() + 1))
    }

    pub(crate) fn next_task_id(&self, rtxn: &RoTxn) -> Result<TaskId> {
        Ok(self.last_task_id(rtxn)?.unwrap_or_default())
    }

    pub(crate) fn get_task(&self, rtxn: &RoTxn, task_id: TaskId) -> Result<Option<Task>> {
        Ok(self.all_tasks.get(rtxn, &BEU32::new(task_id))?)
    }

    /// Convert an iterator to a `Vec` of tasks. The tasks MUST exist or a
    /// `CorruptedTaskQueue` error will be throwed.
    pub(crate) fn get_existing_tasks(
        &self,
        rtxn: &RoTxn,
        tasks: impl IntoIterator<Item = TaskId>,
    ) -> Result<Vec<Task>> {
        tasks
            .into_iter()
            .map(|task_id| {
                self.get_task(rtxn, task_id)
                    .and_then(|task| task.ok_or(Error::CorruptedTaskQueue))
            })
            .collect::<Result<_>>()
    }

    pub(crate) fn update_task(&self, wtxn: &mut RwTxn, task: &Task) -> Result<()> {
        let old_task = self
            .get_task(wtxn, task.uid)?
            .ok_or(Error::CorruptedTaskQueue)?;

        debug_assert_eq!(old_task.uid, task.uid);

        if old_task == *task {
            return Ok(());
        }

        if old_task.status != task.status {
            self.update_status(wtxn, old_task.status, |bitmap| {
                bitmap.remove(task.uid);
            })?;
            self.update_status(wtxn, task.status, |bitmap| {
                bitmap.insert(task.uid);
            })?;
        }

        if old_task.kind.as_kind() != task.kind.as_kind() {
            self.update_kind(wtxn, old_task.kind.as_kind(), |bitmap| {
                bitmap.remove(task.uid);
            })?;
            self.update_kind(wtxn, task.kind.as_kind(), |bitmap| {
                bitmap.insert(task.uid);
            })?;
        }

        self.all_tasks.put(wtxn, &BEU32::new(task.uid), task)?;
        Ok(())
    }

    /// Returns the whole set of tasks that belongs to this index.
    pub(crate) fn index_tasks(&self, rtxn: &RoTxn, index: &str) -> Result<RoaringBitmap> {
        Ok(self.index_tasks.get(rtxn, index)?.unwrap_or_default())
    }

    pub(crate) fn put_index(
        &self,
        wtxn: &mut RwTxn,
        index: &str,
        bitmap: &RoaringBitmap,
    ) -> Result<()> {
        Ok(self.index_tasks.put(wtxn, index, bitmap)?)
    }

    pub(crate) fn update_index(
        &self,
        wtxn: &mut RwTxn,
        index: &str,
        f: impl Fn(&mut RoaringBitmap),
    ) -> Result<()> {
        let mut tasks = self.index_tasks(wtxn, index)?;
        f(&mut tasks);
        self.put_index(wtxn, index, &tasks)?;

        Ok(())
    }

    pub(crate) fn get_status(&self, rtxn: &RoTxn, status: Status) -> Result<RoaringBitmap> {
        Ok(self.status.get(rtxn, &status)?.unwrap_or_default())
    }

    pub(crate) fn put_status(
        &self,
        wtxn: &mut RwTxn,
        status: Status,
        bitmap: &RoaringBitmap,
    ) -> Result<()> {
        Ok(self.status.put(wtxn, &status, bitmap)?)
    }

    pub(crate) fn update_status(
        &self,
        wtxn: &mut RwTxn,
        status: Status,
        f: impl Fn(&mut RoaringBitmap),
    ) -> Result<()> {
        let mut tasks = self.get_status(wtxn, status)?;
        f(&mut tasks);
        self.put_status(wtxn, status, &tasks)?;

        Ok(())
    }

    pub(crate) fn get_kind(&self, rtxn: &RoTxn, kind: Kind) -> Result<RoaringBitmap> {
        Ok(self.kind.get(rtxn, &kind)?.unwrap_or_default())
    }

    pub(crate) fn put_kind(
        &self,
        wtxn: &mut RwTxn,
        kind: Kind,
        bitmap: &RoaringBitmap,
    ) -> Result<()> {
        Ok(self.kind.put(wtxn, &kind, bitmap)?)
    }

    pub(crate) fn update_kind(
        &self,
        wtxn: &mut RwTxn,
        kind: Kind,
        f: impl Fn(&mut RoaringBitmap),
    ) -> Result<()> {
        let mut tasks = self.get_kind(wtxn, kind)?;
        f(&mut tasks);
        self.put_kind(wtxn, kind, &tasks)?;

        Ok(())
    }
}
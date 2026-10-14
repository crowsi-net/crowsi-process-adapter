//! Owner-selected budgets. Mechanism installation remains with pinned processkit.
use crate::Error;
use processkit::ProcessGroupOptions;

/// Whole-tree memory/process/CPU limits and per-file bytes, without a new wire protocol.
/// None means that axis is not requested. This is not a total directory/disk quota.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProcessBudget {
    pub max_memory_bytes: Option<u64>,
    pub max_processes: Option<u32>,
    pub cpu_quota: Option<f64>,
    /// Inherited per-process file-size hard/soft cap; zero allows no file growth.
    pub max_file_bytes: Option<u64>,
}
impl ProcessBudget {
    pub(crate) fn validate(self) -> Result<(), Error> {
        if self == Self::default()
            || self
                .max_memory_bytes
                .is_some_and(|bytes| bytes == 0 || bytes == u64::MAX)
            || self.max_processes == Some(0)
            || self
                .cpu_quota
                .is_some_and(|cpu| !cpu.is_finite() || cpu <= 0.0)
            || self.max_file_bytes == Some(u64::MAX)
        {
            return Err(Error::InvalidLaunch);
        }
        Ok(())
    }
    pub(crate) fn options(self, mut options: ProcessGroupOptions) -> ProcessGroupOptions {
        if let Some(bytes) = self.max_memory_bytes {
            options = options.max_memory(bytes);
        }
        if let Some(processes) = self.max_processes {
            options = options.max_processes(processes);
        }
        if let Some(cpu) = self.cpu_quota {
            options = options.cpu_quota(cpu);
        }
        options
    }
}

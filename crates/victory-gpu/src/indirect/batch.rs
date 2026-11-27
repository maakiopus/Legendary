//! Clamps of many executions in one pass: the arguments of every indirect
//! execution between two points where they may change are bounded together,
//! at the first of them.
//!
//! The executions that come after it are not known yet when the pass is
//! recorded, so it reads what to clamp from a table of jobs in upload
//! memory, which the CPU goes on filling while the list is recorded: the
//! pass is as wide as the table and its threads past the count do nothing.
//! An execution that asks for a record already in the table (the title's
//! tile classes) reads the same clamped one.
//!
//! What the records charge the list's budget is as for records clamped
//! alone, times the executions that read them.

use windows::Win32::Graphics::Direct3D12::*;

use super::{Clamp, uav_barrier};
use crate::host::transition;

/// The jobs of one pass.
pub(crate) const JOBS: u32 = 256;
/// The bytes of its table in upload memory: the count, then the jobs.
pub(crate) const TABLE: u64 = 16 + 16 * JOBS as u64;
/// The bytes of a clamped record's place in the scratch buffer.
pub(crate) const SLOT: u32 = 32;
/// The source buffers a pass can read.
pub(crate) const SOURCES: usize = 4;
/// The most executions charged by one job: the most a record costs (2^26)
/// as often still fits 32 bits.
pub(crate) const TIMES: u32 = 31;

pub(crate) const SOURCE: &str = include_str!("../../shaders/indirect_batch.hlsl");

/// The words of a record of `kind` (PC argument numbering).
pub(crate) fn words(kind: u32) -> Option<u32> {
    super::min_stride(kind).map(|bytes| bytes / 4)
}

/// A job's first word.
pub(crate) fn job(source: u32, kind: u32, times: u32) -> u32 {
    source | (kind << 4) | (words(kind).unwrap_or(0) << 8) | (times << 16)
}

/// The jobs of a pass as the CPU fills them ([`SOURCE`]), and where it
/// keeps which record is where.
#[derive(Default)]
pub(crate) struct Table {
    /// The table in upload memory ([`TABLE`] bytes).
    at: usize,
    /// The place of the first clamped record in the scratch buffer.
    base: u32,
    /// The host copies it can read, by number ([`crate::shadow::Shadow::id`];
    /// 0: none).
    sources: [u64; SOURCES],
    /// How many executions each job is charged for so far.
    times: Vec<u32>,
    /// The job of a record (source, offset, kind).
    known: std::collections::HashMap<(u32, u32, u32), u32>,
}

impl Table {
    /// A table at `at`, with no job yet.
    ///
    /// # Safety
    /// `at` is [`TABLE`] bytes of mapped upload memory, 4-byte aligned, that
    /// live until the list that reads them is recorded and is not run
    /// before.
    pub(crate) unsafe fn new(at: *mut u8, base: u32, sources: [u64; SOURCES]) -> Table {
        // SAFETY: the caller's.
        unsafe { at.cast::<u32>().write_volatile(0) };
        Table { at: at as usize, base, sources, ..Table::default() }
    }

    /// The place in the scratch buffer of the record of `kind` at `offset`
    /// of the host copy `id`, once the pass has run: a job of it is added if
    /// there is none. `None` if the pass cannot read that copy or has no
    /// room for another job.
    pub(crate) fn place(&mut self, id: u64, offset: u64, kind: u32) -> Option<u64> {
        let source = self.sources.iter().position(|&s| s == id)? as u32;
        let offset = u32::try_from(offset).ok().filter(|o| o.is_multiple_of(4))?;
        offset.checked_add(words(kind)? * 4)?;
        let key = (source, offset, kind);
        let table = self.at as *mut u32;
        let place = |i: u32| u64::from(self.base + i * SLOT);
        if let Some(&i) = self.known.get(&key)
            && self.times[i as usize] < TIMES
        {
            self.times[i as usize] += 1;
            // SAFETY: the job's own word of the table ([`Table::new`]).
            unsafe { table.add(4 + 4 * i as usize).write_volatile(job(source, kind, self.times[i as usize])) };
            return Some(place(i));
        }
        let i = self.times.len() as u32;
        if i == JOBS {
            return None;
        }
        self.times.push(1);
        self.known.insert(key, i);
        // SAFETY: inside the table: fewer than [`JOBS`] jobs before it.
        unsafe {
            let to = table.add(4 + 4 * i as usize);
            to.write_volatile(job(source, kind, 1));
            to.add(1).write_volatile(offset);
            to.add(2).write_volatile(self.base + i * SLOT);
            to.add(3).write_volatile(0);
            table.write_volatile(i + 1);
        }
        Some(place(i))
    }
}

/// Records the pass over the table at `jobs` (a host address) into `dst`,
/// which is in state `before` and left in `INDIRECT_ARGUMENT`: after every
/// earlier shader write (the title's argument passes), before what reads
/// the clamped records.
pub(super) fn record(
    c: &Clamp,
    l: &ID3D12GraphicsCommandList,
    (dst, before): (&ID3D12Resource, D3D12_RESOURCE_STATES),
    budget: u64,
    sources: [u64; SOURCES],
    jobs: u64,
) {
    let after = transition(
        dst,
        D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
        D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
    );
    // SAFETY: an open list; our root signature and pipeline; live buffers:
    // the sources and the table are read in the common state (promoted).
    unsafe {
        l.ResourceBarrier(&[
            uav_barrier(),
            transition(dst, before, D3D12_RESOURCE_STATE_UNORDERED_ACCESS),
        ]);
        l.SetComputeRootSignature(&c.root);
        l.SetPipelineState(&c.pso);
        for (i, source) in sources.into_iter().enumerate() {
            l.SetComputeRootShaderResourceView(i as u32, source);
        }
        l.SetComputeRootShaderResourceView(SOURCES as u32, jobs);
        l.SetComputeRootUnorderedAccessView(SOURCES as u32 + 1, dst.GetGPUVirtualAddress());
        l.SetComputeRootUnorderedAccessView(SOURCES as u32 + 2, budget);
        l.Dispatch(JOBS.div_ceil(64), 1, 1);
        // Nothing else the pass writes is read before the next full wait
        // (the budget, by the next pass): only the records have to be there.
        l.ResourceBarrier(&[after]);
    }
}

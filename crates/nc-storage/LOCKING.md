# Storage Locking Contract

The current ATA PIO, AHCI, and NVMe block-device entry points are synchronous and task-context-only.
They must not be called from timer, keyboard, mouse, exception, NMI, or other interrupt handlers.
The current interrupt handlers in `nc-arch/src/idt.rs` only update the timer counter, enqueue input
bytes, or acknowledge the PIC; they do not call storage APIs.

The controller locks serialize access to shared controller runtime state and DMA bounce buffers.
They are held while a command is submitted and polled to completion. This is acceptable only while
these APIs remain task-context-only on the current single-CPU execution model. The locks are not
NMI-safe. Do not add allocation, filesystem work, or synchronous disk I/O to an interrupt handler.

A future interrupt-driven or SMP storage implementation must replace long polling critical sections
with request queues and completion handling, define a global lock order, and add per-CPU interrupt
nesting/debug assertions before advertising IRQ-safe storage access.

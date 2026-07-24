//! mini-rtos: un kernel preemptivo minimalista para Cortex-M.
//!
//! Características:
//! - Hasta `task::MAX_TASKS` tareas con pila propia y estática (sin heap).
//! - Scheduling round-robin apoyado en SysTick (quantum configurable).
//! - `sleep_ticks()` para bloquear una tarea N ticks.
//! - Cambio de contexto vía PendSV (prioridad mínima, no bloquea IRQs).
//!
//! Limitaciones deliberadas (es un RTOS educativo, no de producción):
//! - Sin mutexes/semáforos/colas todavía (fácil de añadir sobre esta base).
//! - Sin protección de stack overflow (podrías añadir un watermark/guard).
//! - Sin soporte de FPU lazy-stacking (usa `thumbv7em-none-eabihf` con cuidado
//!   si tus tareas usan floats: habría que extender el guardado de contexto).

pub mod context_switch;
pub mod scheduler;
pub mod task;

pub use scheduler::{register_task, sleep_ticks};
pub use task::{init_task_stack, TaskState, MAX_TASKS, TASK_STACK_SIZE};

/// Arranca el kernel: nunca retorna. Debe llamarse después de registrar
/// todas las tareas con `register_task`.
///
/// # Safety
/// Debe llamarse una única vez, desde `main`, tras registrar >=1 tarea.
pub unsafe fn start_scheduler() -> ! {
    context_switch::start_first_task()
}

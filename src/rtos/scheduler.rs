//! Scheduler round-robin simple.
//!
//! - `SysTick` incrementa el tick del sistema y decide si toca
//!   cambiar de tarea, disparando `PendSV` cuando corresponde.
//! - `PendSV` (en context_switch.rs) hace el cambio de contexto real
//!   (guardar/restaurar registros), delegando en este módulo sólo la
//!   decisión de "cuál es la siguiente tarea".

use crate::rtos::task::{TaskControlBlock, TaskState, MAX_TASKS};
use core::sync::atomic::{AtomicUsize, Ordering};
use cortex_m::interrupt;
use cortex_m::peripheral::SCB;

/// Tabla estática de tareas. `MAX_TASKS` slots fijos: nada de heap,
/// apto para sistemas sin allocator.
pub static mut TASKS: [TaskControlBlock; MAX_TASKS] =
    [TaskControlBlock::empty(); MAX_TASKS];

/// Índice de la tarea actualmente en ejecución.
pub static CURRENT_TASK: AtomicUsize = AtomicUsize::new(0);

/// Cuántas tareas están registradas hasta ahora.
static TASK_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Cuántos ticks de SysTick dura un "quantum" (slice de tiempo) por tarea.
const TICKS_PER_QUANTUM: u32 = 10;
static QUANTUM_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Registra una nueva tarea ya con su stack inicializado
/// (ver `task::init_task_stack`). Debe llamarse antes de `start()`.
///
/// # Safety
/// Debe llamarse antes de iniciar el scheduler (un solo hilo de
/// ejecución activo), por eso el acceso a `TASKS`/estáticos mutables
/// es seguro aquí.
pub unsafe fn register_task(initial_sp: *mut u32) -> Result<usize, ()> {
    let count = TASK_COUNT.load(Ordering::Relaxed);
    if count >= MAX_TASKS {
        return Err(());
    }

    TASKS[count] = TaskControlBlock {
        stack_pointer: initial_sp,
        state: TaskState::Ready,
        sleep_ticks: 0,
        id: count,
    };

    TASK_COUNT.store(count + 1, Ordering::Relaxed);
    Ok(count)
}

/// Duerme la tarea actual durante `ticks` invocaciones de SysTick.
/// Debe llamarse sólo desde dentro de una tarea (contexto de hilo).
pub fn sleep_ticks(ticks: u32) {
    interrupt::free(|_| unsafe {
        let idx = CURRENT_TASK.load(Ordering::Relaxed);
        TASKS[idx].sleep_ticks = ticks;
        TASKS[idx].state = TaskState::Blocked;
    });
    // Cede la CPU inmediatamente en lugar de esperar al próximo quantum.
    request_context_switch();
    cortex_m::asm::isb();
}

/// Llamado desde el handler de SysTick en cada tick del sistema.
/// Decrementa contadores de sleep y decide si toca cambiar de tarea.
pub fn on_systick() {
    interrupt::free(|_| unsafe {
        // Despertar tareas cuyo sleep haya expirado.
        for task in TASKS.iter_mut() {
            if task.state == TaskState::Blocked && task.sleep_ticks > 0 {
                task.sleep_ticks -= 1;
                if task.sleep_ticks == 0 {
                    task.state = TaskState::Ready;
                }
            }
        }
    });

    let count = QUANTUM_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    if count as u32 >= TICKS_PER_QUANTUM {
        QUANTUM_COUNTER.store(0, Ordering::Relaxed);
        request_context_switch();
    }
}

/// Dispara PendSV (prioridad más baja) para que el cambio de contexto
/// ocurra "cuando el CPU tenga un hueco", sin bloquear otras IRQs.
pub fn request_context_switch() {
    SCB::set_pendsv();
}

/// Algoritmo de selección: round-robin simple entre las tareas en
/// estado `Ready`. Se llama desde dentro de PendSV (context_switch.rs)
/// con interrupciones ya deshabilitadas.
///
/// # Safety
/// Sólo debe invocarse desde el handler de PendSV.
pub unsafe fn select_next_task() -> usize {
    let count = TASK_COUNT.load(Ordering::Relaxed);
    if count == 0 {
        // No hay tareas registradas: quedarse en la 0 (idle/panic).
        return 0;
    }

    let current = CURRENT_TASK.load(Ordering::Relaxed);

    // Marca la tarea saliente como Ready si seguía corriendo (no bloqueada).
    if TASKS[current].state == TaskState::Running {
        TASKS[current].state = TaskState::Ready;
    }

    let mut next = current;
    for _ in 0..count {
        next = (next + 1) % count;
        if TASKS[next].state == TaskState::Ready {
            break;
        }
    }

    TASKS[next].state = TaskState::Running;
    CURRENT_TASK.store(next, Ordering::Relaxed);
    next
}

/// Devuelve el puntero de pila guardado de la tarea dada.
///
/// # Safety
/// `idx` debe ser un índice válido dentro de `TASKS`.
pub unsafe fn task_stack_pointer(idx: usize) -> *mut u32 {
    TASKS[idx].stack_pointer
}

/// Actualiza el puntero de pila guardado de la tarea dada.
///
/// # Safety
/// `idx` debe ser un índice válido dentro de `TASKS`.
pub unsafe fn set_task_stack_pointer(idx: usize, sp: *mut u32) {
    TASKS[idx].stack_pointer = sp;
}

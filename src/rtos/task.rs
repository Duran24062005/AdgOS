//! Representación de una tarea (Task Control Block) y la construcción
//! de su marco de pila inicial, para que el primer cambio de contexto
//! la "engañe" como si ya hubiera sido interrumpida una vez.

/// Número máximo de tareas soportadas simultáneamente.
pub const MAX_TASKS: usize = 8;

/// Tamaño del stack de cada tarea, en palabras de 32 bits.
/// 256 palabras = 1 KB. Ajusta según lo que necesite cada tarea.
pub const TASK_STACK_SIZE: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskState {
    /// La tarea existe y puede ser planificada.
    Ready,
    /// La tarea es la que se está ejecutando actualmente.
    Running,
    /// La tarea está dormida/bloqueada y no debe planificarse.
    Blocked,
    /// Slot vacío / tarea terminada.
    Unused,
}

/// Bloque de control de tarea (TCB).
#[derive(Clone, Copy)]
pub struct TaskControlBlock {
    /// Puntero de pila guardado de la tarea (se actualiza en cada
    /// cambio de contexto). Es lo único que el scheduler necesita
    /// tocar directamente en la mayoría de los casos.
    pub stack_pointer: *mut u32,
    pub state: TaskState,
    /// Contador de ticks restantes si la tarea está dormida (sleep).
    pub sleep_ticks: u32,
    pub id: usize,
}

// SAFETY: el TCB se comparte entre el contexto de interrupción (PendSV/SysTick)
// y el hilo principal, pero el acceso siempre ocurre con interrupciones
// deshabilitadas (ver `critical_section` en scheduler.rs), así que no hay
// condiciones de carrera reales.
unsafe impl Send for TaskControlBlock {}
unsafe impl Sync for TaskControlBlock {}

impl TaskControlBlock {
    pub const fn empty() -> Self {
        TaskControlBlock {
            stack_pointer: core::ptr::null_mut(),
            state: TaskState::Unused,
            sleep_ticks: 0,
            id: 0,
        }
    }
}

/// Construye el marco de pila inicial de una tarea nueva, de forma que
/// cuando el `PendSV` handler haga su `pop`/`bx lr` habitual, la CPU
/// termine saltando a `entry_point` con `xPSR.T=1` (modo Thumb) y las
/// interrupciones habilitadas.
///
/// Layout del stack para Cortex-M (sin FPU) que el hardware espera
/// encontrar al hacer `exception return`, de arriba hacia abajo:
///   xPSR, PC, LR, R12, R3, R2, R1, R0   <- apilado por el hardware
///   R11..R4                             <- apilado "a mano" por nosotros
///
/// # Safety
/// `stack_mem` debe ser un buffer válido, alineado a 8 bytes, y vivir
/// durante toda la vida de la tarea (normalmente `static mut`).
pub unsafe fn init_task_stack(
    stack_mem: &mut [u32; TASK_STACK_SIZE],
    entry_point: extern "C" fn() -> !,
) -> *mut u32 {
    // La pila crece hacia abajo: empezamos en el último elemento.
    let mut sp = stack_mem.as_mut_ptr().add(TASK_STACK_SIZE);

    // --- Marco "apilado por hardware" en una excepción real ---
    sp = sp.sub(1);
    core::ptr::write(sp, 0x0100_0000); // xPSR: bit Thumb (T) en 1

    sp = sp.sub(1);
    core::ptr::write(sp, entry_point as usize as u32); // PC: entry point de la tarea

    sp = sp.sub(1);
    core::ptr::write(sp, task_return_trap as usize as u32); // LR: a dónde volver si la tarea retorna

    sp = sp.sub(1);
    core::ptr::write(sp, 0x1212_1212); // R12

    sp = sp.sub(1);
    core::ptr::write(sp, 0x0303_0303); // R3
    sp = sp.sub(1);
    core::ptr::write(sp, 0x0202_0202); // R2
    sp = sp.sub(1);
    core::ptr::write(sp, 0x0101_0101); // R1
    sp = sp.sub(1);
    core::ptr::write(sp, 0x0000_0000); // R0 (parámetro de entrada, si se usara)

    // --- Marco "apilado por software" (nuestro propio PendSV) ---
    // R11..R4
    for reg_val in [
        0x1111_1111u32,
        0x1010_1010,
        0x0909_0909,
        0x0808_0808,
        0x0707_0707,
        0x0606_0606,
        0x0505_0505,
        0x0404_0404,
    ] {
        sp = sp.sub(1);
        core::ptr::write(sp, reg_val);
    }

    sp
}

/// Si una tarea retorna (no debería, las tareas son `-> !`), caemos
/// aquí y nos quedamos detenidos en vez de ejecutar memoria basura.
extern "C" fn task_return_trap() -> ! {
    loop {
        cortex_m::asm::bkpt();
    }
}

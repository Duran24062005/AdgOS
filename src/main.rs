#![no_std]
#![no_main]

use cortex_m::peripheral::syst::SystClkSource;
use cortex_m_rt::{entry, exception};
use panic_halt as _;

mod rtos;

use rtos::task::TASK_STACK_SIZE;

// --- Pilas estáticas de cada tarea -----------------------------------
// `static mut` porque no hay heap; cada tarea necesita su propio buffer
// que viva para siempre. En un proyecto real podrías envolver esto en
// un `MaybeUninit` o usar la crate `static_cell` para mayor seguridad.
static mut STACK_TASK_A: [u32; TASK_STACK_SIZE] = [0; TASK_STACK_SIZE];
static mut STACK_TASK_B: [u32; TASK_STACK_SIZE] = [0; TASK_STACK_SIZE];
static mut STACK_TASK_IDLE: [u32; TASK_STACK_SIZE] = [0; TASK_STACK_SIZE];

/// Tarea de ejemplo A: parpadea "algo" cada 500 ticks (ajusta a tu HW).
extern "C" fn task_a() -> ! {
    loop {
        // Aquí iría, por ejemplo: gpio.set_high(); toggle de un LED, etc.
        // Este RTOS no asume ningún HAL concreto, así que dejamos el punto
        // de extensión marcado con un comentario.
        cortex_m::asm::nop();
        rtos::sleep_ticks(500);
    }
}

/// Tarea de ejemplo B: hace otra cosa cada 200 ticks.
extern "C" fn task_b() -> ! {
    loop {
        cortex_m::asm::nop();
        rtos::sleep_ticks(200);
    }
}

/// Tarea idle: se ejecuta cuando ninguna otra tarea está lista.
/// Siempre debe existir al menos una tarea en estado Ready, o el
/// scheduler no tiene a dónde saltar.
extern "C" fn task_idle() -> ! {
    loop {
        cortex_m::asm::wfi(); // duerme la CPU hasta la próxima interrupción
    }
}

#[entry]
fn main() -> ! {
    let mut cp = cortex_m::Peripherals::take().unwrap();

    // Registrar las tareas ANTES de arrancar el scheduler.
    unsafe {
        let sp_a = rtos::init_task_stack(&mut STACK_TASK_A, task_a);
        let sp_b = rtos::init_task_stack(&mut STACK_TASK_B, task_b);
        let sp_idle = rtos::init_task_stack(&mut STACK_TASK_IDLE, task_idle);

        rtos::register_task(sp_a).ok();
        rtos::register_task(sp_b).ok();
        rtos::register_task(sp_idle).ok();
    }

    // Configurar SysTick como el "corazón" del scheduler.
    // Ajusta la recarga según la frecuencia real de tu núcleo; el
    // ejemplo asume 16 MHz de reloj de sistema -> tick cada 1 ms.
    let syst = &mut cp.SYST;
    syst.set_clock_source(SystClkSource::Core);
    syst.set_reload(16_000 - 1); // 1 ms @ 16 MHz
    syst.clear_current();
    syst.enable_counter();
    syst.enable_interrupt();

    // PendSV debe tener la prioridad MÁS BAJA de todo el sistema, para
    // que nunca interrumpa a una IRQ de mayor prioridad a mitad camino.
    unsafe {
        cp.SCB.set_priority(cortex_m::peripheral::scb::SystemHandler::PendSV, 0xFF);
    }

    // Arrancar el scheduler: carga la tarea 0 y nunca vuelve aquí.
    unsafe { rtos::start_scheduler() }
}

/// Handler de SysTick: avanza el tick del sistema y decide si toca
/// cambiar de tarea (delegado enteramente en el módulo scheduler).
#[exception]
fn SysTick() {
    rtos::scheduler::on_systick();
}

// El handler de PendSV (donde ocurre el cambio de contexto real) está
// definido directamente en `rtos::context_switch::PendSV` con
// `#[no_mangle]`. cortex-m-rt lo enlaza automáticamente porque el
// nombre del símbolo coincide exactamente con el vector de la tabla de
// excepciones (sobrescribe el `DefaultHandler` débil). No hace falta
// nada más aquí.

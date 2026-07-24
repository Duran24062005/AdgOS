//! Cambio de contexto real, en ensamblador ARM Thumb, dentro del
//! handler de la excepción `PendSV`.
//!
//! Cortex-M, al entrar a cualquier excepción, ya guarda automáticamente
//! en la pila activa: xPSR, PC, LR, R12, R3, R2, R1, R0. Nosotros sólo
//! necesitamos guardar a mano lo que falta: R4-R11. Luego cambiamos el
//! puntero de pila (PSP) a la tarea siguiente y restauramos R4-R11 desde
//! su pila, dejando que el hardware haga el resto al retornar de la
//! excepción (`bx lr` con el valor mágico de EXC_RETURN).

use crate::rtos::scheduler::{select_next_task, set_task_stack_pointer, task_stack_pointer};
use core::arch::naked_asm;

/// Handler de la excepción PendSV, enlazado DIRECTAMENTE por nombre en
/// la tabla de vectores de cortex-m-rt (símbolo débil `PendSV`
/// sobrescrito aquí). Por eso va `#[no_mangle]` y NO se envuelve en
/// ninguna otra función: una función "naked" no debe llamarse nunca
/// desde código Rust normal, porque no tiene prólogo/epílogo y asume
/// que entra exactamente con el estado que deja el hardware al tomar
/// la excepción.
///
/// # Safety
/// Sólo el hardware, al tomar la excepción PendSV, debe "invocar" esto.
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn PendSV() {
    naked_asm!(
        // 1) Leer PSP (Process Stack Pointer): es la pila de la tarea
        //    que estaba corriendo, ya que las tareas usan PSP y las
        //    excepciones/handler usan MSP (configurado en main.rs).
        "mrs r0, psp",

        // 2) Guardar R4-R11 (software-saved) en la pila de la tarea saliente.
        "stmdb r0!, {{r4-r11}}",

        // 3) Guardar el nuevo puntero de pila (ya con R4-R11 apilados)
        //    en el TCB de la tarea saliente. r0 = &mut sp saliente.
        "bl {save_sp}",

        // 4) Elegir la siguiente tarea (actualiza CURRENT_TASK).
        "bl {select_next}",

        // 5) Cargar el puntero de pila guardado de la tarea entrante.
        "bl {load_sp}",
        // load_sp devuelve el puntero en r0.

        // 6) Restaurar R4-R11 desde la pila de la tarea entrante.
        "ldmia r0!, {{r4-r11}}",

        // 7) Actualizar PSP con el nuevo puntero (después de sacar R4-R11,
        //    r0 ya apunta al frame que el hardware sabe desapilar solo).
        "msr psp, r0",

        // 8) Asegurar que el pipeline vea el cambio antes de retornar.
        "isb",

        // 9) Retornar de la excepción. EXC_RETURN = 0xFFFFFFFD indica
        //    "vuelve a modo hilo, usando PSP, sin FPU extendido".
        "ldr lr, ={exc_return}",
        "bx lr",

        save_sp = sym save_outgoing_sp,
        select_next = sym select_next_task_asm,
        load_sp = sym load_incoming_sp,
        exc_return = const 0xFFFF_FFFDu32,
    );
}

/// Wrapper con firma "extern C" simple para llamar desde asm: recibe en
/// r0 el SP saliente (post R4-R11) y lo guarda en el TCB de la tarea actual.
#[no_mangle]
extern "C" fn save_outgoing_sp(sp: *mut u32) {
    unsafe {
        let idx = crate::rtos::scheduler::CURRENT_TASK.load(core::sync::atomic::Ordering::Relaxed);
        set_task_stack_pointer(idx, sp);
    }
}

/// Wrapper para invocar `select_next_task` desde asm (sin argumentos).
#[no_mangle]
extern "C" fn select_next_task_asm() {
    unsafe {
        select_next_task();
    }
}

/// Devuelve en r0 (valor de retorno) el SP de la tarea ya seleccionada
/// como `CURRENT_TASK`.
#[no_mangle]
extern "C" fn load_incoming_sp() -> *mut u32 {
    unsafe {
        let idx = crate::rtos::scheduler::CURRENT_TASK.load(core::sync::atomic::Ordering::Relaxed);
        task_stack_pointer(idx)
    }
}

/// Arranca el scheduler: carga la pila de la primera tarea en PSP,
/// cambia a modo "thread usa PSP" y salta a ella. Se invoca UNA vez
/// desde `main`, nunca retorna.
///
/// # Safety
/// Debe llamarse sólo una vez, con al menos una tarea ya registrada.
#[unsafe(naked)]
pub unsafe extern "C" fn start_first_task() -> ! {
    naked_asm!(
        // r0 = puntero de pila de la tarea 0 (ya "pre-armado" por init_task_stack)
        "bl {load_sp}",
        // Saltar los 8 registros "software" (R4-R11) que ya están
        // en el frame inicial fabricado por init_task_stack.
        "ldmia r0!, {{r4-r11}}",
        "msr psp, r0",
        "isb",
        // Cambiar CONTROL para que modo hilo use PSP en vez de MSP.
        "mrs r0, control",
        "orr r0, r0, #2",
        "msr control, r0",
        "isb",
        // Retornar "a la tarea": desapilamos el frame de hardware
        // manualmente saltando vía el mismo mecanismo de exception
        // return no es necesario aquí porque no estamos en excepción;
        // en su lugar, hacemos pop manual de los registros de hardware
        // y saltamos al PC de la tarea.
        "pop {{r0-r3}}",
        "pop {{r4}}",      // r12 (usamos r4 como temporal)
        "mov r12, r4",
        "pop {{r4}}",      // lr
        "mov lr, r4",
        "pop {{r4}}",      // pc de la tarea -> lo guardamos y saltamos al final
        "pop {{r5}}",      // xpsr (descartado, ya estamos en thread mode thumb)
        "bx r4",
        load_sp = sym load_incoming_sp,
    );
}

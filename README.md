# AdgOS - Mini-rtos 

Un RTOS preemptivo mínimo para microcontroladores **ARM Cortex-M**, escrito
en Rust `no_std` / `no_main`, sin heap y sin dependencias más allá de
`cortex-m`, `cortex-m-rt` y `panic-halt`.

## ⚠️ Aviso importante sobre esta entrega

Este código lo escribí siguiendo el patrón estándar y bien documentado de
cambio de contexto para Cortex-M (PSP/MSP + PendSV + SysTick), el mismo
principio que usan FreeRTOS, ChibiOS, RTIC, etc. **No pude compilarlo ni
probarlo en hardware real ni en QEMU** en este entorno, porque el sandbox
no tiene acceso de red para instalar el toolchain de Rust ni el target
`thumbv7em-none-eabihf`. Antes de darlo por bueno en producción:

1. Compílalo tú (`cargo build`) y revisa cualquier error de sintaxis.
2. Pruébalo primero en QEMU (`qemu-system-arm -M lm3s6965evb ...`) o en tu
   placa real con un depurador (probe-rs, OpenOCD + gdb) antes de confiar
   en él para nada crítico.
3. Las funciones "naked" (`PendSV`, `start_first_task`) usan la API
   **estable** de naked functions (`#[unsafe(naked)]` + `naked_asm!`),
   disponible en Rust recientes. Si tu toolchain es más antiguo, tendrás
   que adaptarlas a la sintaxis vieja de nightly (`#[naked]` + `asm!` con
   `options(noreturn)`).

## Qué incluye

- **Scheduler round-robin** con quantum configurable (`TICKS_PER_QUANTUM`
  en `src/rtos/scheduler.rs`).
- **Cambio de contexto** completo (guarda/restaura R4-R11 a mano; el resto
  lo hace el hardware) en `src/rtos/context_switch.rs`.
- **`sleep_ticks(n)`**: una tarea puede dormirse N ticks de SysTick y
  cede la CPU inmediatamente.
- Hasta `MAX_TASKS` tareas (por defecto 8), cada una con su propio stack
  estático — nada de heap, apto para sistemas sin `alloc`.
- Tres tareas de demo en `src/main.rs` (`task_a`, `task_b`, `task_idle`).

## Qué NO incluye (a propósito, para mantenerlo pequeño)

- Mutexes, semáforos o colas entre tareas (la base ya está para añadirlos).
- Protección de stack overflow (podrías añadir un "watermark" al final de
  cada stack y comprobarlo periódicamente).
- Soporte de FPU / lazy stacking — si usas `-eabihf` y tus tareas usan
  `f32`/`f64`, tendrás que extender `context_switch.rs` para guardar
  también S16-S31.
- Prioridades entre tareas (todas son iguales, round-robin puro).

## Estructura

```
mini-rtos/
├── Cargo.toml
├── memory.x              <- AJUSTA a la memoria real de tu chip
├── .cargo/config.toml     <- AJUSTA el target y el chip del runner
└── src/
    ├── main.rs            <- demo + configuración de SysTick/PendSV
    └── rtos/
        ├── mod.rs             <- API pública del kernel
        ├── task.rs            <- TCB + construcción del stack inicial
        ├── scheduler.rs       <- selección de siguiente tarea, sleep, tick
        └── context_switch.rs  <- asm de PendSV y arranque de la 1ª tarea
```

## Cómo adaptarlo a tu microcontrolador

1. **`memory.x`**: cambia `ORIGIN`/`LENGTH` de `FLASH` y `RAM` según el
   datasheet de tu chip.
2. **`.cargo/config.toml`**: cambia `target` según tu núcleo
   (`thumbv6m-none-eabi` para M0/M0+, `thumbv7em-none-eabihf` para M4F,
   etc.) y el `chip` del `runner` de `probe-rs`.
3. **`src/main.rs`**:
   - Ajusta `syst.set_reload(...)` a la frecuencia real de tu núcleo
     para obtener el tick que quieras (el ejemplo asume 16 MHz → 1 ms).
   - Reemplaza las tareas de ejemplo por tu lógica real (control de
     GPIOs, lectura de sensores, comunicación serie, etc.), usando el
     HAL de tu chip (`stm32f4xx-hal`, `rp2040-hal`, `nrf52840-hal`...).
4. Añade `rustup target add <tu-target>` y las dependencias del HAL que
   uses.

## Build

```bash
rustup target add thumbv7em-none-eabihf
cargo build --release
```

## Cómo funciona el cambio de contexto (resumen)

1. Cada tarea corre en **modo hilo usando PSP** (Process Stack Pointer);
   el kernel/handlers usan **MSP** (Main Stack Pointer). Esto los aísla.
2. `SysTick` incrementa el tick del sistema, despierta tareas dormidas
   cuyo `sleep_ticks` llegó a 0, y cada `TICKS_PER_QUANTUM` ticks marca
   pendiente una interrupción `PendSV` (`SCB::set_pendsv()`).
3. `PendSV` tiene la **prioridad más baja** posible, así nunca interrumpe
   a otras IRQs a mitad camino — se ejecuta "cuando el CPU tiene un hueco".
4. Dentro de `PendSV`: el hardware ya guardó R0-R3, R12, LR, PC, xPSR de
   la tarea saliente en su propio stack (PSP) al entrar a la excepción.
   Nosotros completamos guardando R4-R11 a mano, guardamos el puntero de
   pila resultante en el TCB, elegimos la siguiente tarea, cargamos su
   puntero de pila guardado, restauramos R4-R11, y hacemos que el
   hardware haga el resto al "retornar de la excepción" (`bx lr` con el
   código mágico `EXC_RETURN`), lo cual desapila R0-R3/R12/LR/PC/xPSR de
   la tarea entrante automáticamente.
5. La primera vez que una tarea corre, su stack se "pre-fabrica" con
   `init_task_stack()` para que tenga exactamente esa misma forma, como
   si ya hubiera sido interrumpida una vez — así el mecanismo de arriba
   funciona igual para tareas nuevas y para tareas ya en marcha.

## Extensión sugerida: un mutex simple

Un buen siguiente paso, con esta base, es añadir un mutex tipo
"priority-less spinlock + bloqueo cooperativo":

```rust
pub struct Mutex { locked: core::sync::atomic::AtomicBool }

impl Mutex {
    pub fn lock(&self) {
        while self.locked.swap(true, core::sync::atomic::Ordering::Acquire) {
            crate::rtos::sleep_ticks(1); // cede la CPU en vez de busy-wait puro
        }
    }
    pub fn unlock(&self) {
        self.locked.store(false, core::sync::atomic::Ordering::Release);
    }
}
```

No es óptimo (no hay wait-queues reales), pero es un punto de partida
correcto sobre el que iterar.

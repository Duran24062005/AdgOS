/* Ajusta ORIGIN y LENGTH a la memoria real de tu microcontrolador.
   Los valores de abajo son un ejemplo típico para un STM32F407
   (1 MB Flash, 128 KB RAM). Cámbialos según tu datasheet. */
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 1024K
  RAM   : ORIGIN = 0x20000000, LENGTH = 128K
}

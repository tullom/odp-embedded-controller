/* ##### EMBASSY NOTE
    Originally from https://github.com/embassy-rs/teleprobe/blob/main/link_ram_cortex_m.x
    Adjusted to put everything in RAM (run-from-RAM under teleprobe).
*/

/* Provides information about the memory layout of the device */
INCLUDE memory_teleprobe.x

/* # Entry point = reset vector */
EXTERN(__RESET_VECTOR);
EXTERN(Reset);
ENTRY(Reset);

/* # Exception vectors */
EXTERN(__EXCEPTIONS);

EXTERN(DefaultHandler);

PROVIDE(NonMaskableInt = DefaultHandler);
EXTERN(HardFaultTrampoline);
PROVIDE(MemoryManagement = DefaultHandler);
PROVIDE(BusFault = DefaultHandler);
PROVIDE(UsageFault = DefaultHandler);
PROVIDE(SecureFault = DefaultHandler);
PROVIDE(SVCall = DefaultHandler);
PROVIDE(DebugMonitor = DefaultHandler);
PROVIDE(PendSV = DefaultHandler);
PROVIDE(SysTick = DefaultHandler);

PROVIDE(DefaultHandler = DefaultHandler_);
PROVIDE(HardFault = HardFault_);

/* # Interrupt vectors */
EXTERN(__INTERRUPTS);

/* # Pre-initialization function */
PROVIDE(__pre_init = DefaultPreInit);

/* # Sections */
SECTIONS
{
  PROVIDE(_ram_start = ORIGIN(RAM));
  PROVIDE(_ram_end = ORIGIN(RAM) + LENGTH(RAM));
  PROVIDE(_stack_start = _ram_end);

  /* ## Sections in RAM */
  /* ### Vector table */
  .vector_table ORIGIN(RAM) :
  {
    __vector_table = .;

    /* Initial Stack Pointer (SP) value, masked to 8-byte alignment. */
    LONG(_stack_start & 0xFFFFFFF8);

    /* Reset vector */
    KEEP(*(.vector_table.reset_vector));

    /* Exceptions */
    __exceptions = .;
    KEEP(*(.vector_table.exceptions));
    __eexceptions = .;

    /* Device specific interrupts */
    KEEP(*(.vector_table.interrupts));
  } > RAM

  PROVIDE(_stext = ADDR(.vector_table) + SIZEOF(.vector_table));

  /* ### .text */
  .text _stext :
  {
    __stext = .;
    *(.Reset);

    *(.text .text.*);

    *(.HardFaultTrampoline);
    *(.HardFault.*);

    . = ALIGN(4);
    __etext = .;
  } > RAM

  /* ### .rodata */
  .rodata : ALIGN(4)
  {
    . = ALIGN(4);
    __srodata = .;
    *(.rodata .rodata.*);

    . = ALIGN(4);
    __erodata = .;
  } > RAM

  /* ### .data
   *
   * Critical: when running from RAM, .data must be loaded as part of the ELF
   * (LMA == VMA, both in RAM). cortex-m-rt's Reset handler copies bytes in
   * the range `__sdata..__edata` from the LMA to the VMA. We want it to copy
   * **nothing** (the loader has already placed our initialized data in RAM at
   * the correct VMA), so we set `__sdata == __edata == start_of_section` and
   * place the actual `.data .data.*` content AFTER both markers.
   */
  .data : ALIGN(4)
  {
    . = ALIGN(4);
    __sdata = .;
    __edata = .;

    *(.data .data.*);
    . = ALIGN(4);
  } > RAM

  /* LMA of .data — must be in RAM so the ELF loader writes the magic
   * string for `_SEGGER_RTT` (and every other initialized static) directly
   * into RAM. This is what makes RTT work under run-from-RAM. */
  __sidata = LOADADDR(.data);

  /* ### .gnu.sgstubs (Cortex-M TrustZone-M veneers) */
  .gnu.sgstubs : ALIGN(32)
  {
    . = ALIGN(32);
    __veneer_base = .;
    *(.gnu.sgstubs*)
    . = ALIGN(32);
  } > RAM
  . = ALIGN(32);
  __veneer_limit = .;

  /* ### .bss */
  .bss (NOLOAD) : ALIGN(4)
  {
    . = ALIGN(4);
    __sbss = .;
    *(.bss .bss.*);
    *(COMMON);
    . = ALIGN(4);
  } > RAM
  . = ALIGN(4);
  __ebss = .;

  /* ### .uninit */
  .uninit (NOLOAD) : ALIGN(4)
  {
    . = ALIGN(4);
    __suninit = .;
    *(.uninit .uninit.*);
    . = ALIGN(4);
    __euninit = .;
  } > RAM

  /* Place the heap right after `.uninit` in RAM */
  PROVIDE(__sheap = __euninit);

  /* ## .got */
  .got (NOLOAD) :
  {
    KEEP(*(.got .got.*));
  }

  /* ## Discarded sections */
  /DISCARD/ :
  {
    *(.ARM.exidx);
    *(.ARM.exidx.*);
    *(.ARM.extab.*);
  }
}

/* # Alignment checks */
ASSERT(ORIGIN(RAM) % 4 == 0, "
ERROR(cortex-m-rt): the start of the RAM region must be 4-byte aligned");

ASSERT(__sdata % 4 == 0 && __edata % 4 == 0, "
BUG(cortex-m-rt): .data is not 4-byte aligned");

ASSERT(__sidata % 4 == 0, "
BUG(cortex-m-rt): the LMA of .data is not 4-byte aligned");

ASSERT(__sbss % 4 == 0 && __ebss % 4 == 0, "
BUG(cortex-m-rt): .bss is not 4-byte aligned");

ASSERT(__sheap % 4 == 0, "
BUG(cortex-m-rt): start of .heap is not 4-byte aligned");

ASSERT(_stack_start % 8 == 0, "
ERROR(cortex-m-rt): stack start address is not 8-byte aligned.");

/* # Position checks */
ASSERT(__exceptions == ADDR(.vector_table) + 0x8, "
BUG(cortex-m-rt): the reset vector is missing");

ASSERT(__eexceptions == ADDR(.vector_table) + 0x40, "
BUG(cortex-m-rt): the exception vectors are missing");

ASSERT(SIZEOF(.vector_table) > 0x40, "
ERROR(cortex-m-rt): The interrupt vectors are missing.");

ASSERT(ADDR(.vector_table) + SIZEOF(.vector_table) <= _stext, "
ERROR(cortex-m-rt): The .text section can't be placed inside the .vector_table section");

ASSERT(_stext + SIZEOF(.text) < ORIGIN(RAM) + LENGTH(RAM), "
ERROR(cortex-m-rt): The .text section must be placed inside the RAM memory.");

/* # Other checks */
ASSERT(SIZEOF(.got) == 0, "
ERROR(cortex-m-rt): .got section detected in the input object files
Dynamic relocations are not supported.");

/* Provides weak aliases (cf. PROVIDED) for device specific interrupt handlers */
INCLUDE device.x

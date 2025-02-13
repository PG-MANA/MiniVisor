OUTPUT_FORMAT(elf64-aarch64)
ENTRY(main)

__ALIGN_SIZE = 0x1000;
__RAM_BASE   = 0x40400000;

SECTIONS {
  . = __RAM_BASE + SIZEOF_HEADERS;

  .text : ALIGN(__ALIGN_SIZE) {
    *(.text .text.*)
  }

  .data : ALIGN(__ALIGN_SIZE) {
    *(.data .data.*)
  }

  .rodata : ALIGN(__ALIGN_SIZE) {
    *(.rodata .rodata.*)
  }

  .bss : ALIGN(__ALIGN_SIZE) {
    *(.bss .bss.*)
  }

  .got : ALIGN(__ALIGN_SIZE) {
    *(.got .got.*)
  }
}

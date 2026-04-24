/* MEMORY layout for run-from-RAM under teleprobe.
 *
 * Mirrors the RAM region of `memory.x` (the regular flash-based layout) but
 * omits FLASH entirely — under teleprobe the binary is loaded directly into
 * RAM and executed there, so no flash-resident sections are emitted.
 *
 * The first 0x3000 bytes of RAM are reserved (per the chip's reset behaviour
 * documented in `memory.x`), so we start the teleprobe RAM region at the same
 * offset.
 */
MEMORY {
    RAM   : ORIGIN = 0x20003000, LENGTH = 228K
}

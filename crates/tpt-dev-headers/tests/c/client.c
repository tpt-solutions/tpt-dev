/*
 * Compile-only client for the header generated from
 * crates/tpt-dev-ffi/tests/c_library.rs. CI runs
 *   gcc -std=c99 -Wall -Werror -c client.c
 * against the generated counter.h. The program is never linked or run.
 */
#include "counter.h"

int main(void) {
    Counter *c = counter_create();
    const uint8_t *bytes = NULL;
    size_t len = 0;
    uint32_t ticks = 0;

    if (c == NULL) {
        return 1;
    }
    if (counter_open(c) != 0 || counter_start(c) != 0) {
        counter_destroy(c);
        return 1;
    }
    counter_tick(c);
    if (counter_read(c, &ticks) != 0) {
        counter_destroy(c);
        return 1;
    }
    if (counter_ticks_bytes(c, &bytes, &len) != 0) {
        counter_destroy(c);
        return 1;
    }
    counter_stop(c);
    counter_close(c);
    counter_destroy(c);
    return (int)ticks;
}

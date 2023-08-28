#include <stdint.h>

#define ARGBUF_LEN 65536

// Define the argument buffer used to communicate between contract and host
uint8_t A[ARGBUF_LEN];

// ==== Host functions ====
//
// These functions are provided by the host. See `piecrust-uplink` for a full
// list. Here we only declare the ones we will need.

extern uint32_t c(
    uint8_t *contract_id,
    uint8_t *fn_name,
    uint32_t fn_name_len,
    uint32_t fn_arg_len,
    uint64_t points_limit
);

extern uint32_t hd(
    uint8_t *name,
    uint32_t name_len
);

inline static void memcpy(void *target, void *source, uint32_t len) {
    uint8_t *t = (uint8_t*) target;
    uint8_t *s = (uint8_t*) source;

    for (uint32_t i = 0; i < len; ++i) {
        t[i] = s[i];
    }
}

// ==== Helper functions ====
//
// These will help us write the exported functions underneath

// Reads a contract ID from the argument buffer
inline static void read_contract_id(uint8_t id[32]) {
    memcpy(id, A, 32);
}

// Calls the counter contract to increment the counter
inline static void increment_counter(uint8_t contract_id[32]) {
    uint32_t fn_name_len = 9;
    uint8_t fn_name[9] = "increment";
    c(contract_id, fn_name, fn_name_len, 0, 0);
}

// Reads a 64-bit from the argument buffer
inline static void read_integer(int64_t *i) {
    memcpy(i, A, 8);
}

// Writes a 64-bit integer to the argument buffer
inline static void write_integer(int64_t i) {
    memcpy(A, &i, 8);
}

// Calls the counter contract to read the counter
inline static int64_t read_counter(uint8_t contract_id[32]) {
    uint32_t fn_name_len = 10;
    uint8_t fn_name[10] = "read_value";

    c(contract_id, fn_name, fn_name_len, 0, 0);

    int64_t i;
    read_integer(&i);
    return i;
}

// ==== Exported functions ====

// Increments and reads the counter contract. The function expects the counter
// contract ID to be written to the argument buffer before being called.
int32_t increment_and_read(int32_t _arg_len) {
    uint8_t counter_id[32];
    read_contract_id(counter_id);
    increment_counter(counter_id);

    int64_t i = read_counter(counter_id);
    write_integer(i);
    return 8;
}

// Calls the "hd" extern with an (almost) certainly out of bounds pointer, in an
// effort to trigger an error.
int32_t out_of_bounds(int32_t _arg_len) {
    hd((uint8_t*)4294967295, 2);
    return 0;
}

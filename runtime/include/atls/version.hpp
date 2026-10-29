#pragma once

/**
 * @file
 * @brief The identifier of this set of runtime headers.
 *
 * `atlc` computes it from the other headers' text and writes it into every header it generates, which
 * refuses to compile against a runtime with another identifier: write the runtime out with
 * `atlc build --emit-runtime DIR` from the same `atlc` that generated the strategy.
 */

/** @brief The runtime's compatibility identifier. */
#define ATLS_RUNTIME_ID 0xb6f021227b1b

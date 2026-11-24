#pragma once

#include "./data.hpp"
#include "./expr.hpp"
#include "./orders.hpp"
#include "./window.hpp"

/**
 * @file
 * @brief What compiled strategies call: rings, the built-in indicators, the three-valued operators on
 *   undefined (NaN) values, the target rule and how a target is priced.
 *
 * Generated code holds no arithmetic on indices or buffers of its own: it is all here, tested once
 * (`runtime/tests/`).
 *
 * This header includes the runtime's parts, and generated code includes it (a program with `measures {}` also
 * includes `measures.hpp`): `expr.hpp` (the undefined value, checked integer arithmetic, the three-valued operators,
 * `ATLS_NO_FMA` and `atls::atl`), `window.hpp` (rings and the built-in indicators), `orders.hpp` (the target rule and
 * its pricing) and `data.hpp` (the record and the tape of `data {}`).
 */

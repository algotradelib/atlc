#pragma once

#include <algotradelib/strategy>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <limits>
#include <sstream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <type_traits>

/**
 * @file
 * @brief The expressions of compiled strategies: the undefined value, decimals as doubles, checked integer arithmetic
 *   and the three-valued operators on undefined (NaN) values.
 *
 * It also holds what the other runtime headers share: `ATLS_NO_FMA` and the alias `atls::atl`.
 */

/**
 * @brief No fused multiply-add in the body that starts with it: `s += x * x` rounds twice, whatever the
 *   includer's flags (Clang fuses by default on ARM64).
 *
 * GCC and MSVC have no such pragma: build with `-ffp-contract=off` (GCC) or `/fp:precise` (MSVC).
 */
#if defined(__clang__)
#define ATLS_NO_FMA _Pragma("clang fp contract(off)")
#else
#define ATLS_NO_FMA
#endif

namespace atls {

namespace atl = algotradelib;

namespace ops {

/** @brief The undefined value. */
inline constexpr double NaN = std::numeric_limits<double>::quiet_NaN();

/** @brief Whether `T` is a complete type where this is instantiated: false for a class only declared. */
template <class T, class = void>
struct is_complete : std::false_type {};

/** @brief Whether `T` is a complete type where this is instantiated: true for a class defined. */
template <class T>
struct is_complete<T, decltype(void(sizeof(T)))> : std::true_type {};

/**
 * @brief An exact decimal as the nearest double: the coefficient divided once by 10^scale.
 * @param value The decimal.
 * @return The double.
 */
inline double to_double(const atl::Decimal64& value) {
  double divisor = 1;
  for (std::uint8_t step = 0; step < value.scale(); ++step) {
    divisor *= 10;
  }
  return static_cast<double>(value.coefficient()) / divisor;
}

/**
 * @brief A number as text, for error messages.
 * @param value The number.
 * @return Its text.
 */
template <typename T>
std::string text(T value) {
  std::ostringstream out;
  out << value;
  return out.str();
}

/**
 * @brief A parameter value that must be a whole number.
 * @param name The parameter, for the message.
 * @param value The value.
 * @return The value as an integer.
 * @throws std::invalid_argument If the value is not a whole number of magnitude at most 9e15.
 */
inline std::int64_t whole(std::string_view name, double value) {
  if (!std::isfinite(value) || value != std::floor(value) || std::fabs(value) > 9e15) {
    throw std::invalid_argument(std::string(name) + " is an int; " + text(value) + " is not a whole number");
  }
  return static_cast<std::int64_t>(value);
}

// Integer arithmetic: an `int` is always defined, so a result that does not fit in 64 bits cannot become NaN. It
// throws instead, and the strategy stops rather than trading on a wrong number (C++'s signed overflow is undefined
// behaviour, so each operation is checked before it runs).

/**
 * @brief Throws for an integer operation whose result does not fit in 64 bits.
 * @param a The left operand.
 * @param op The operator.
 * @param b The right operand.
 * @throws std::overflow_error Always.
 */
[[noreturn]] inline void overflow(std::int64_t a, const char* op, std::int64_t b) {
  throw std::overflow_error("atls: integer overflow in " + text(a) + " " + op + " " + text(b));
}

/**
 * @brief `a + b`.
 * @throws std::overflow_error If the sum does not fit in 64 bits.
 */
inline std::int64_t add_int(std::int64_t a, std::int64_t b) {
  if (
    (b > 0 && a > std::numeric_limits<std::int64_t>::max() - b) ||
    (b < 0 && a < std::numeric_limits<std::int64_t>::min() - b)
  ) {
    overflow(a, "+", b);
  }
  return a + b;
}

/**
 * @brief `a - b`.
 * @throws std::overflow_error If the difference does not fit in 64 bits.
 */
inline std::int64_t sub_int(std::int64_t a, std::int64_t b) {
  if (
    (b < 0 && a > std::numeric_limits<std::int64_t>::max() + b) ||
    (b > 0 && a < std::numeric_limits<std::int64_t>::min() + b)
  ) {
    overflow(a, "-", b);
  }
  return a - b;
}

/**
 * @brief `a * b`.
 * @throws std::overflow_error If the product does not fit in 64 bits.
 */
inline std::int64_t mul_int(std::int64_t a, std::int64_t b) {
  constexpr std::int64_t max = std::numeric_limits<std::int64_t>::max();
  constexpr std::int64_t min = std::numeric_limits<std::int64_t>::min();
  const bool over = a > 0 ? (b > 0 ? a > max / b : b < min / a) : (b > 0 ? a < min / b : (a != 0 && b < max / a));
  if (over) {
    overflow(a, "*", b);
  }
  return a * b;
}

/**
 * @brief `-a`.
 * @throws std::overflow_error For the smallest 64-bit integer, whose negation does not fit.
 */
inline std::int64_t neg_int(std::int64_t a) {
  if (a == std::numeric_limits<std::int64_t>::min()) {
    overflow(0, "-", a);
  }
  return -a;
}

/**
 * @brief `abs(a)`.
 * @throws std::overflow_error For the smallest 64-bit integer, whose magnitude does not fit.
 */
inline std::int64_t abs_int(std::int64_t a) {
  return a < 0 ? neg_int(a) : a;
}

// Three-valued operators on doubles: NaN is undefined, a bool is 0 or 1.

/** @brief `a / b`, undefined when `b` is 0. */
inline double div(double a, double b) {
  return b == 0 ? NaN : a / b;
}
/** @brief `a < b`, undefined when either is. */
inline double lt(double a, double b) {
  return std::isnan(a) || std::isnan(b) ? NaN : (a < b ? 1.0 : 0.0);
}
/** @brief `a <= b`, undefined when either is. */
inline double le(double a, double b) {
  return std::isnan(a) || std::isnan(b) ? NaN : (a <= b ? 1.0 : 0.0);
}
/** @brief `a > b`, undefined when either is. */
inline double gt(double a, double b) {
  return std::isnan(a) || std::isnan(b) ? NaN : (a > b ? 1.0 : 0.0);
}
/** @brief `a >= b`, undefined when either is. */
inline double ge(double a, double b) {
  return std::isnan(a) || std::isnan(b) ? NaN : (a >= b ? 1.0 : 0.0);
}
/** @brief `a == b`, undefined when either is. */
inline double eq(double a, double b) {
  return std::isnan(a) || std::isnan(b) ? NaN : (a == b ? 1.0 : 0.0);
}
/** @brief `a != b`, undefined when either is. */
inline double ne(double a, double b) {
  return std::isnan(a) || std::isnan(b) ? NaN : (a != b ? 1.0 : 0.0);
}

/** @brief Three-valued `and`: a defined false decides it. */
inline double and3(double a, double b) {
  if ((!std::isnan(a) && a == 0) || (!std::isnan(b) && b == 0)) {
    return 0;
  }
  return std::isnan(a) || std::isnan(b) ? NaN : 1.0;
}

/** @brief Three-valued `or`: a defined true decides it. */
inline double or3(double a, double b) {
  if ((!std::isnan(a) && a != 0) || (!std::isnan(b) && b != 0)) {
    return 1;
  }
  return std::isnan(a) || std::isnan(b) ? NaN : 0.0;
}

/** @brief Three-valued `not`. */
inline double not3(double a) {
  return std::isnan(a) ? NaN : (a == 0 ? 1.0 : 0.0);
}

/** @brief `c ? a : b`, undefined when `c` is. */
inline double pick(double c, double a, double b) {
  return std::isnan(c) ? NaN : (c != 0 ? a : b);
}

/** @brief `min`, undefined when either is. */
inline double min3(double a, double b) {
  return std::isnan(a) || std::isnan(b) ? NaN : std::min(a, b);
}
/** @brief `max`, undefined when either is. */
inline double max3(double a, double b) {
  return std::isnan(a) || std::isnan(b) ? NaN : std::max(a, b);
}

/** @brief -1, 0 or 1; undefined when `x` is. */
inline double sign_of(double x) {
  return std::isnan(x) ? NaN : static_cast<double>((x > 0) - (x < 0));
}
/** @brief Natural logarithm; undefined at or below 0. */
inline double log_of(double x) {
  return x > 0 ? std::log(x) : NaN;
}
/** @brief Square root; undefined below 0 (IEEE's square root of a negative number is NaN). */
inline double sqrt_of(double x) {
  return std::sqrt(x);
}
/** @brief The nearest whole number, halves away from zero (as `std::llround`). */
inline double round_of(double x) {
  return std::round(x);
}
/** @brief Down to a whole number. */
inline double floor_of(double x) {
  return std::floor(x);
}

} // namespace ops
} // namespace atls

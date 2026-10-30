#pragma once

#include <algotradelib/strategy>

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <sstream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#if defined(ATL_HAS_EIGEN)
#include <Eigen/Dense>
#endif

/**
 * @file
 * @brief What compiled strategies call: rings, the built-in indicators, the three-valued operators on
 *   undefined (NaN) values, the target rule and how a target is priced.
 *
 * Generated code holds no arithmetic on indices or buffers of its own: it is all here, tested once
 * (`runtime/tests/`). A value without enough history is NaN; under `missing = restart` an undefined input
 * restarts the window (the caller clears the ring and calls `reset()`).
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
  if ((b > 0 && a > std::numeric_limits<std::int64_t>::max() - b) || (b < 0 && a < std::numeric_limits<std::int64_t>::min() - b)) {
    overflow(a, "+", b);
  }
  return a + b;
}

/**
 * @brief `a - b`.
 * @throws std::overflow_error If the difference does not fit in 64 bits.
 */
inline std::int64_t sub_int(std::int64_t a, std::int64_t b) {
  if ((b < 0 && a > std::numeric_limits<std::int64_t>::max() + b) || (b > 0 && a < std::numeric_limits<std::int64_t>::min() + b)) {
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
inline std::int64_t abs_int(std::int64_t a) { return a < 0 ? neg_int(a) : a; }

// Three-valued operators on doubles: NaN is undefined, a bool is 0 or 1.

/** @brief `a / b`, undefined when `b` is 0. */
inline double div(double a, double b) { return b == 0 ? NaN : a / b; }
/** @brief `a < b`, undefined when either is. */
inline double lt(double a, double b) { return std::isnan(a) || std::isnan(b) ? NaN : (a < b ? 1.0 : 0.0); }
/** @brief `a <= b`, undefined when either is. */
inline double le(double a, double b) { return std::isnan(a) || std::isnan(b) ? NaN : (a <= b ? 1.0 : 0.0); }
/** @brief `a > b`, undefined when either is. */
inline double gt(double a, double b) { return std::isnan(a) || std::isnan(b) ? NaN : (a > b ? 1.0 : 0.0); }
/** @brief `a >= b`, undefined when either is. */
inline double ge(double a, double b) { return std::isnan(a) || std::isnan(b) ? NaN : (a >= b ? 1.0 : 0.0); }
/** @brief `a == b`, undefined when either is. */
inline double eq(double a, double b) { return std::isnan(a) || std::isnan(b) ? NaN : (a == b ? 1.0 : 0.0); }
/** @brief `a != b`, undefined when either is. */
inline double ne(double a, double b) { return std::isnan(a) || std::isnan(b) ? NaN : (a != b ? 1.0 : 0.0); }

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
inline double not3(double a) { return std::isnan(a) ? NaN : (a == 0 ? 1.0 : 0.0); }

/** @brief `c ? a : b`, undefined when `c` is. */
inline double pick(double c, double a, double b) { return std::isnan(c) ? NaN : (c != 0 ? a : b); }

/** @brief `min`, undefined when either is. */
inline double min3(double a, double b) { return std::isnan(a) || std::isnan(b) ? NaN : std::min(a, b); }
/** @brief `max`, undefined when either is. */
inline double max3(double a, double b) { return std::isnan(a) || std::isnan(b) ? NaN : std::max(a, b); }

/** @brief -1, 0 or 1; undefined when `x` is. */
inline double sign_of(double x) { return std::isnan(x) ? NaN : static_cast<double>((x > 0) - (x < 0)); }
/** @brief Natural logarithm; undefined at or below 0. */
inline double log_of(double x) { return x > 0 ? std::log(x) : NaN; }
/** @brief Square root; undefined below 0 (IEEE's square root of a negative number is NaN). */
inline double sqrt_of(double x) { return std::sqrt(x); }
/** @brief The nearest whole number, halves away from zero (as `std::llround`). */
inline double round_of(double x) { return std::round(x); }
/** @brief Down to a whole number. */
inline double floor_of(double x) { return std::floor(x); }

/** @brief The last values of one input since its last restart. `ago(0)` is the newest. */
class Ring {
public:
  /**
   * @brief Sets the capacity and empties the ring.
   * @param capacity How many values it holds (at least 1).
   */
  void reset(std::size_t capacity) {
    buffer_.assign(std::max<std::size_t>(capacity, 1), 0.0);
    clear();
  }

  /** @brief Forgets every value: the window restarts. */
  void clear() {
    seen_ = 0;
    head_ = 0;
  }

  /**
   * @brief Adds the newest value.
   * @param value The value.
   */
  void push(double value) {
    head_ = head_ + 1 == capacity() ? 0 : head_ + 1;
    buffer_[head_] = value;
    ++seen_;
  }

  /**
   * @brief The value `k` pushes ago.
   * @param k Pushes ago; `k < capacity()` and `k < seen()`.
   * @return The value.
   */
  double ago(std::size_t k) const { return buffer_[head_ >= k ? head_ - k : head_ + capacity() - k]; }

  /** @brief Values pushed since the last restart. */
  std::size_t seen() const { return seen_; }

  /** @brief How many values the ring holds. */
  std::size_t capacity() const { return buffer_.size(); }

private:
  std::vector<double> buffer_;
  std::size_t head_{0};
  std::size_t seen_{0};
};

/** @brief A window length, set at run time. */
struct Window {
  /**
   * @brief Sets the length.
   * @param n The length.
   */
  void set(std::size_t n) { n_ = n; }
  /** @brief The length. */
  std::size_t n() const { return n_; }
  /** @brief The length. */
  std::size_t n_{1};
};

/** @brief Simple moving average. Incremental: a ring of at least n + 1; recompute: at least n. */
struct Sma : Window {
  /** @brief Restarts. */
  void reset() { sum_ = 0; }
  /**
   * @brief After the ring took the newest value: a running sum.
   * @param ring The input's ring.
   * @return The mean, or NaN before n values.
   */
  double update(const Ring& ring) {
    const std::size_t n = this->n();
    sum_ += ring.ago(0);
    if (ring.seen() > n) {
      sum_ -= ring.ago(n);
    }
    return ring.seen() >= n ? sum_ / static_cast<double>(n) : NaN;
  }
  /**
   * @brief After the ring took the newest value: the window summed again, oldest first.
   * @param ring The input's ring.
   * @return The mean, or NaN before n values.
   */
  double recompute(const Ring& ring) const {
    const std::size_t n = this->n();
    if (ring.seen() < n) {
      return NaN;
    }
    double sum = 0;
    for (std::size_t k = n; k-- > 0;) {
      sum += ring.ago(k);
    }
    return sum / static_cast<double>(n);
  }
  /** @brief The running sum. */
  double sum_{0};
};

/** @brief Sum over the window. Same ring needs as `Sma`. */
struct TsSum : Window {
  /** @brief Restarts. */
  void reset() { sum_ = 0; }
  /**
   * @brief Running sum.
   * @param ring The input's ring.
   * @return The sum, or NaN before n values.
   */
  double update(const Ring& ring) {
    const std::size_t n = this->n();
    sum_ += ring.ago(0);
    if (ring.seen() > n) {
      sum_ -= ring.ago(n);
    }
    return ring.seen() >= n ? sum_ : NaN;
  }
  /**
   * @brief Summed again, oldest first.
   * @param ring The input's ring.
   * @return The sum, or NaN before n values.
   */
  double recompute(const Ring& ring) const {
    const std::size_t n = this->n();
    if (ring.seen() < n) {
      return NaN;
    }
    double sum = 0;
    for (std::size_t k = n; k-- > 0;) {
      sum += ring.ago(k);
    }
    return sum;
  }
  /** @brief The running sum. */
  double sum_{0};
};

/** @brief The value n bars ago. A ring of at least n + 1. */
struct Lag : Window {
  /** @brief Nothing to restart beyond the ring. */
  void reset() {}
  /**
   * @brief Reads the ring.
   * @param ring The input's ring.
   * @return The value n pushes ago, or NaN before n + 1 values.
   */
  double update(const Ring& ring) const { return ring.seen() > this->n() ? ring.ago(this->n()) : NaN; }
  /**
   * @brief The same: a lag has nothing to recompute.
   * @param ring The input's ring.
   * @return As `update`.
   */
  double recompute(const Ring& ring) const { return update(ring); }
};

/** @brief Cutler's RSI over the last n moves, 50 when nothing moved. Incremental: a ring of n + 2. */
struct Rsi : Window {
  /** @brief Restarts. */
  void reset() {
    up_ = 0;
    down_ = 0;
  }
  /**
   * @brief Running sums of up and down moves.
   * @param ring The input's ring.
   * @return The RSI, or NaN before n + 1 values.
   */
  double update(const Ring& ring) {
    ATLS_NO_FMA
    const std::size_t n = this->n();
    if (ring.seen() >= 2) {
      const double d = ring.ago(0) - ring.ago(1);
      (d > 0 ? up_ : down_) += std::fabs(d);
    }
    if (ring.seen() >= n + 2) {
      const double d = ring.ago(n) - ring.ago(n + 1);
      (d > 0 ? up_ : down_) -= std::fabs(d);
    }
    if (ring.seen() < n + 1) {
      return NaN;
    }
    return up_ + down_ == 0 ? 50 : 100 * up_ / (up_ + down_);
  }
  /**
   * @brief The n moves summed again, oldest first (a ring of n + 1).
   * @param ring The input's ring.
   * @return The RSI, or NaN before n + 1 values.
   */
  double recompute(const Ring& ring) const {
    ATLS_NO_FMA
    const std::size_t n = this->n();
    if (ring.seen() < n + 1) {
      return NaN;
    }
    double up = 0;
    double down = 0;
    for (std::size_t k = n; k-- > 0;) {
      const double d = ring.ago(k) - ring.ago(k + 1);
      (d > 0 ? up : down) += std::fabs(d);
    }
    return up + down == 0 ? 50 : 100 * up / (up + down);
  }
  /** @brief The running sum of up moves. */
  double up_{0};
  /** @brief The running sum of down moves. */
  double down_{0};
};

/**
 * @brief Sample standard deviation over the window, n - 1 in the divisor.
 *
 * Incremental: running sums of the values and their squares over a ring of at least n + 1,
 * `sqrt((ss - s * s / n) / (n - 1))`; recompute: two passes over a ring of at least n (the mean, then the
 * squared deviations).
 */
struct Stdev : Window {
  /** @brief Restarts. */
  void reset() {
    sum_ = 0;
    squares_ = 0;
  }
  /**
   * @brief After the ring took the newest value.
   * @param ring The input's ring.
   * @return The deviation, or NaN before n values.
   */
  double update(const Ring& ring) {
    ATLS_NO_FMA
    const std::size_t n = this->n();
    const double x = ring.ago(0);
    sum_ += x;
    squares_ += x * x;
    if (ring.seen() > n) {
      const double old = ring.ago(n);
      sum_ -= old;
      squares_ -= old * old;
    }
    if (ring.seen() < n) {
      return NaN;
    }
    const double nd = static_cast<double>(n);
    return std::sqrt((squares_ - sum_ * sum_ / nd) / (nd - 1));
  }
  /**
   * @brief Two passes over the window, oldest first.
   * @param ring The input's ring.
   * @return The deviation, or NaN before n values.
   */
  double recompute(const Ring& ring) const {
    ATLS_NO_FMA
    const std::size_t n = this->n();
    if (ring.seen() < n) {
      return NaN;
    }
    double sum = 0;
    for (std::size_t k = n; k-- > 0;) {
      sum += ring.ago(k);
    }
    const double mean = sum / static_cast<double>(n);
    double squares = 0;
    for (std::size_t k = n; k-- > 0;) {
      const double d = ring.ago(k) - mean;
      squares += d * d;
    }
    return std::sqrt(squares / (static_cast<double>(n) - 1));
  }
  /** @brief The running sum. */
  double sum_{0};
  /** @brief The running sum of squares. */
  double squares_{0};
};

/**
 * @brief `(x - mean) / sd` over the window (sample deviation), undefined when the deviation is 0.
 *   Incremental: running sums as `Stdev`; recompute: two passes.
 */
struct Zscore : Window {
  /** @brief Restarts. */
  void reset() { sd_.reset(); }
  /**
   * @brief Sets the length.
   * @param n The length.
   */
  void set(std::size_t n) {
    Window::set(n);
    sd_.set(n);
  }
  /**
   * @brief After the ring took the newest value.
   * @param ring The input's ring.
   * @return The z-score, or NaN before n values or on a flat window.
   */
  double update(const Ring& ring) {
    const double sd = sd_.update(ring);
    if (!(sd > 0)) {
      return NaN;
    }
    return (ring.ago(0) - sd_.sum_ / static_cast<double>(this->n())) / sd;
  }
  /**
   * @brief Two passes over the window.
   * @param ring The input's ring.
   * @return The z-score, or NaN before n values or on a flat window.
   */
  double recompute(const Ring& ring) const {
    ATLS_NO_FMA
    const std::size_t n = this->n();
    if (ring.seen() < n) {
      return NaN;
    }
    double sum = 0;
    for (std::size_t k = n; k-- > 0;) {
      sum += ring.ago(k);
    }
    const double mean = sum / static_cast<double>(n);
    double squares = 0;
    for (std::size_t k = n; k-- > 0;) {
      const double d = ring.ago(k) - mean;
      squares += d * d;
    }
    const double sd = std::sqrt(squares / (static_cast<double>(n) - 1));
    return sd > 0 ? (ring.ago(0) - mean) / sd : NaN;
  }
  /** @brief The deviation's running sums. */
  Stdev sd_;
};

/**
 * @brief Population standard deviation over the window, the count in the divisor. Same ring needs and
 *   arithmetic as `Stdev`, `sqrt((ss - s * s / n) / n)`, never below 0.
 */
struct StdevPop : Window {
  /** @brief Restarts. */
  void reset() {
    sum_ = 0;
    squares_ = 0;
  }
  /**
   * @brief After the ring took the newest value.
   * @param ring The input's ring.
   * @return The deviation, or NaN before n values.
   */
  double update(const Ring& ring) {
    ATLS_NO_FMA
    const std::size_t n = this->n();
    const double x = ring.ago(0);
    sum_ += x;
    squares_ += x * x;
    if (ring.seen() > n) {
      const double old = ring.ago(n);
      sum_ -= old;
      squares_ -= old * old;
    }
    if (ring.seen() < n) {
      return NaN;
    }
    const double nd = static_cast<double>(n);
    return std::sqrt(std::max(0.0, (squares_ - sum_ * sum_ / nd) / nd));
  }
  /**
   * @brief Two passes over the window, oldest first.
   * @param ring The input's ring.
   * @return The deviation, or NaN before n values.
   */
  double recompute(const Ring& ring) const {
    ATLS_NO_FMA
    const std::size_t n = this->n();
    if (ring.seen() < n) {
      return NaN;
    }
    double sum = 0;
    for (std::size_t k = n; k-- > 0;) {
      sum += ring.ago(k);
    }
    const double mean = sum / static_cast<double>(n);
    double squares = 0;
    for (std::size_t k = n; k-- > 0;) {
      const double d = ring.ago(k) - mean;
      squares += d * d;
    }
    return std::sqrt(squares / static_cast<double>(n));
  }
  /** @brief The running sum. */
  double sum_{0};
  /** @brief The running sum of squares. */
  double squares_{0};
};

/**
 * @brief Running sums over the values that exist in the last n slots (`missing = skip`).
 *
 * A missing value (NaN) takes its slot in the ring but adds nothing. Defined once the ring has seen n slots
 * and holds at least `min` values. The ring holds at least n + 1 slots.
 */
struct SkipSums : Window {
  /**
   * @brief Sets the fewest values that must exist in the window.
   * @param m The minimum.
   */
  void set_min(std::size_t m) { min_ = m; }
  /** @brief Restarts. */
  void reset() {
    sum_ = 0;
    squares_ = 0;
    count_ = 0;
  }
  /**
   * @brief After the ring took the newest slot.
   * @param ring The input's ring.
   * @param squares Whether to keep the sum of squares too.
   * @return Whether the window is defined.
   */
  bool take(const Ring& ring, bool squares) {
    ATLS_NO_FMA
    const std::size_t n = this->n();
    const double x = ring.ago(0);
    if (!std::isnan(x)) {
      sum_ += x;
      if (squares) {
        squares_ += x * x;
      }
      ++count_;
    }
    if (ring.seen() > n) {
      const double old = ring.ago(n);
      if (!std::isnan(old)) {
        sum_ -= old;
        if (squares) {
          squares_ -= old * old;
        }
        --count_;
      }
    }
    return ring.seen() >= n && count_ >= min_ && count_ > 0;
  }
  /**
   * @brief The values that exist in the last n slots summed again, oldest first. A ring of n will do.
   * @param ring The input's ring.
   * @param sum Set to the sum.
   * @param count Set to how many values exist.
   * @return Whether the window is defined.
   */
  bool resum(const Ring& ring, double& sum, std::size_t& count) const {
    const std::size_t n = this->n();
    sum = 0;
    count = 0;
    if (ring.seen() < n) {
      return false;
    }
    for (std::size_t k = n; k-- > 0;) {
      const double x = ring.ago(k);
      if (!std::isnan(x)) {
        sum += x;
        ++count;
      }
    }
    return count >= min_ && count > 0;
  }
  /** @brief The running sum. */
  double sum_{0};
  /** @brief The running sum of squares. */
  double squares_{0};
  /** @brief How many values exist in the window. */
  std::size_t count_{0};
  /** @brief The fewest values that must exist. */
  std::size_t min_{1};
};

/** @brief `sma(x, n, missing = skip)`: the mean of the values that exist in the last n slots. */
struct SmaSkip : SkipSums {
  /**
   * @brief After the ring took the newest slot.
   * @param ring The input's ring.
   * @return The mean, or NaN when the window is undefined.
   */
  double update(const Ring& ring) { return this->take(ring, false) ? this->sum_ / static_cast<double>(this->count_) : NaN; }
  /**
   * @brief Summed again, oldest first.
   * @param ring The input's ring.
   * @return The mean, or NaN when the window is undefined.
   */
  double recompute(const Ring& ring) const {
    double sum = 0;
    std::size_t count = 0;
    return this->resum(ring, sum, count) ? sum / static_cast<double>(count) : NaN;
  }
};

/** @brief `ts_sum(x, n, missing = skip)`: the sum of the values that exist in the last n slots. */
struct TsSumSkip : SkipSums {
  /**
   * @brief After the ring took the newest slot.
   * @param ring The input's ring.
   * @return The sum, or NaN when the window is undefined.
   */
  double update(const Ring& ring) { return this->take(ring, false) ? this->sum_ : NaN; }
  /**
   * @brief Summed again, oldest first.
   * @param ring The input's ring.
   * @return The sum, or NaN when the window is undefined.
   */
  double recompute(const Ring& ring) const {
    double sum = 0;
    std::size_t count = 0;
    return this->resum(ring, sum, count) ? sum : NaN;
  }
};

/**
 * @brief Standard deviation of the values that exist in the last n slots: divided by their count c (`Pop`,
 *   as Chan's `smartstd`) or by c - 1 (undefined below two values). Never below 0.
 */
template <bool Pop>
struct StdevSkipT : SkipSums {
  /**
   * @brief After the ring took the newest slot.
   * @param ring The input's ring.
   * @return The deviation, or NaN when the window is undefined.
   */
  double update(const Ring& ring) {
    ATLS_NO_FMA
    if (!this->take(ring, true)) {
      return NaN;
    }
    const double c = static_cast<double>(this->count_);
    if (!Pop && this->count_ < 2) {
      return NaN;
    }
    return std::sqrt(std::max(0.0, (this->squares_ - this->sum_ * this->sum_ / c) / (Pop ? c : c - 1)));
  }
  /**
   * @brief Two passes over the values that exist, oldest first: the mean, then the squared deviations.
   * @param ring The input's ring.
   * @return The deviation, or NaN when the window is undefined.
   */
  double recompute(const Ring& ring) const {
    ATLS_NO_FMA
    double sum = 0;
    std::size_t count = 0;
    if (!this->resum(ring, sum, count) || (!Pop && count < 2)) {
      return NaN;
    }
    const double c = static_cast<double>(count);
    const double mean = sum / c;
    double squares = 0;
    for (std::size_t k = this->n(); k-- > 0;) {
      const double x = ring.ago(k);
      if (!std::isnan(x)) {
        const double d = x - mean;
        squares += d * d;
      }
    }
    return std::sqrt(squares / (Pop ? c : c - 1));
  }
};

/** @brief `stdev(x, n, missing = skip)`: sample deviation over the values that exist. */
using StdevSkip = StdevSkipT<false>;

/** @brief `stdev_pop(x, n, missing = skip)`: population deviation over the values that exist. */
using StdevPopSkip = StdevSkipT<true>;

/**
 * @brief `zscore(x, n, missing = skip)`: the newest value against the values that exist; undefined if it is
 *   missing.
 */
struct ZscoreSkip : StdevSkipT<false> {
  /**
   * @brief After the ring took the newest slot.
   * @param ring The input's ring.
   * @return The z-score, or NaN.
   */
  double update(const Ring& ring) {
    const double sd = StdevSkipT<false>::update(ring);
    if (!(sd > 0) || std::isnan(ring.ago(0))) {
      return NaN;
    }
    return (ring.ago(0) - this->sum_ / static_cast<double>(this->count_)) / sd;
  }
  /**
   * @brief Two passes over the values that exist.
   * @param ring The input's ring.
   * @return The z-score, or NaN.
   */
  double recompute(const Ring& ring) const {
    const double sd = StdevSkipT<false>::recompute(ring);
    if (!(sd > 0) || std::isnan(ring.ago(0))) {
      return NaN;
    }
    double sum = 0;
    std::size_t count = 0;
    this->resum(ring, sum, count);
    return (ring.ago(0) - sum / static_cast<double>(count)) / sd;
  }
};

/** @brief `ts_sum(x, n, m)`: the sum of the last n values, defined once m are in. Keeps its own ring. */
struct TsSumMin {
  /**
   * @brief Sets the window and the minimum count.
   * @param n The window.
   * @param m The minimum count.
   */
  void set(std::size_t n, std::size_t m) {
    n_ = n;
    m_ = m;
    ring_.reset(n + 1);
  }
  /** @brief Restarts. */
  void reset() {
    ring_.clear();
    sum_ = 0;
  }
  /**
   * @brief Takes the newest value.
   * @param x The value.
   * @return The sum, or NaN before m values.
   */
  double update(double x) {
    ring_.push(x);
    sum_ += x;
    if (ring_.seen() > n_) {
      sum_ -= ring_.ago(n_);
    }
    return ring_.seen() >= m_ ? sum_ : NaN;
  }
  /** @brief The values. */
  Ring ring_;
  /** @brief The window. */
  std::size_t n_{1};
  /** @brief The minimum count. */
  std::size_t m_{1};
  /** @brief The running sum. */
  double sum_{0};
};

/**
 * @brief Least squares of y on x with an intercept over the last n bars, from running sums of x, y, x * x
 *   and x * y over its own rings.
 */
struct OlsSums {
  /**
   * @brief Sets the window.
   * @param n The window.
   */
  void set(std::size_t n) {
    n_ = n;
    ys_.reset(n + 1);
    xs_.reset(n + 1);
  }
  /** @brief Restarts. */
  void reset() {
    ys_.clear();
    xs_.clear();
    sx_ = sy_ = sxx_ = sxy_ = 0;
  }
  /**
   * @brief Takes the newest pair.
   * @param y The regressand.
   * @param x The regressor.
   * @return Whether the window is full.
   */
  bool push(double y, double x) {
    ATLS_NO_FMA
    ys_.push(y);
    xs_.push(x);
    sx_ += x;
    sy_ += y;
    sxx_ += x * x;
    sxy_ += x * y;
    if (xs_.seen() > n_) {
      const double oy = ys_.ago(n_);
      const double ox = xs_.ago(n_);
      sx_ -= ox;
      sy_ -= oy;
      sxx_ -= ox * ox;
      sxy_ -= ox * oy;
    }
    return xs_.seen() >= n_;
  }
  /** @brief The slope; undefined for a flat x. */
  double slope() const {
    ATLS_NO_FMA
    const double n = static_cast<double>(n_);
    const double d = n * sxx_ - sx_ * sx_;
    return d != 0 ? (n * sxy_ - sx_ * sy_) / d : NaN;
  }
  /** @brief The regressands. */
  Ring ys_;
  /** @brief The regressors. */
  Ring xs_;
  /** @brief The window. */
  std::size_t n_{2};
  /** @brief The running sum of x. */
  double sx_{0};
  /** @brief The running sum of y. */
  double sy_{0};
  /** @brief The running sum of x * x. */
  double sxx_{0};
  /** @brief The running sum of x * y. */
  double sxy_{0};
};

/** @brief `ols_slope(y, x, n)`, incremental. */
struct OlsSlope : OlsSums {
  /**
   * @brief Takes the newest pair.
   * @param y The regressand.
   * @param x The regressor.
   * @return The slope, or NaN.
   */
  double update(double y, double x) { return push(y, x) ? slope() : NaN; }
};

/** @brief `ols_intercept(y, x, n)`, incremental. */
struct OlsIntercept : OlsSums {
  /**
   * @brief Takes the newest pair.
   * @param y The regressand.
   * @param x The regressor.
   * @return The intercept, or NaN.
   */
  double update(double y, double x) {
    ATLS_NO_FMA
    if (!push(y, x)) {
      return NaN;
    }
    const double b = slope();
    return (sy_ - b * sx_) / static_cast<double>(n_);
  }
};

/**
 * @brief A Kalman filter of y on (x, 1) with an identity transition (Chan, Example 3.3).
 *
 * The slope and intercept drift with variance delta / (1 - delta), and y is measured with variance ve.
 * Before each bar's correction it reports the forecast error and its deviation, and after it the slope.
 */
struct KalmanHedge {
  /**
   * @brief Sets delta and ve.
   * @param delta The drift parameter.
   * @param ve The measurement variance.
   */
  void set(double delta, double ve) {
    vw_ = delta / (1 - delta);
    ve_ = ve;
  }
  /** @brief Starts over: a zero state and covariance. */
  void restart() {
    b0_ = b1_ = 0;
    r00_ = r11_ = -vw_;
    r01_ = r10_ = 0;
    e_ = q_ = NaN;
  }
  /**
   * @brief One prediction and correction.
   * @param y The regressand.
   * @param x The regressor.
   */
  void update(double y, double x) {
    ATLS_NO_FMA
    // Predict: R += Vw, the forecast and its variance.
    r00_ += vw_;
    r11_ += vw_;
    const double forecast = x * b0_ + b1_;
    // x' R x with x = (x, 1), row by column.
    const double t0 = x * r00_ + r10_;
    const double t1 = x * r01_ + r11_;
    q_ = (t0 * x + t1) + ve_;
    // Correct: K = R x / Q, beta += K e, R -= K x' R.
    const double k0 = (r00_ * x + r01_) / q_;
    const double k1 = (r10_ * x + r11_) / q_;
    e_ = y - forecast;
    b0_ += k0 * e_;
    b1_ += k1 * e_;
    const double a00 = k0 * t0;
    const double a01 = k0 * t1;
    const double a10 = k1 * t0;
    const double a11 = k1 * t1;
    r00_ -= a00;
    r01_ -= a01;
    r10_ -= a10;
    r11_ -= a11;
  }
  /** @brief The slope after the last correction. */
  double beta() const { return b0_; }
  /** @brief The last forecast error. */
  double error() const { return e_; }
  /** @brief The last forecast error's deviation. */
  double sd() const { return std::sqrt(q_); }

  /** @brief The drift variance. */
  double vw_{0};
  /** @brief The measurement variance. */
  double ve_{0};
  /** @brief The slope. */
  double b0_{0};
  /** @brief The intercept. */
  double b1_{0};
  /** @brief The covariance, row 0, column 0. */
  double r00_{0};
  /** @brief The covariance, row 0, column 1. */
  double r01_{0};
  /** @brief The covariance, row 1, column 0. */
  double r10_{0};
  /** @brief The covariance, row 1, column 1. */
  double r11_{0};
  /** @brief The last forecast error. */
  double e_{NaN};
  /** @brief The last forecast variance. */
  double q_{NaN};
};

#if defined(ATL_HAS_EIGEN)
/**
 * @brief `ols_slope`/`ols_intercept` solved again on every window: the normal equations of [x, 1] by Eigen's
 *   LLT (`--indicators naive`).
 * @tparam Slope The slope if true, else the intercept.
 */
template <bool Slope>
struct OlsLlt {
  /**
   * @brief Sets the window.
   * @param n The window.
   */
  void set(std::size_t n) {
    n_ = n;
    ys_.reset(n + 1);
    xs_.reset(n + 1);
    window_y_.resize(static_cast<Eigen::Index>(n));
    window_x_.resize(static_cast<Eigen::Index>(n));
  }
  /** @brief Restarts. */
  void reset() {
    ys_.clear();
    xs_.clear();
  }
  /**
   * @brief Takes the newest pair.
   * @param y The regressand.
   * @param x The regressor.
   * @return The slope or intercept, or NaN before n pairs.
   */
  double update(double y, double x) {
    ys_.push(y);
    xs_.push(x);
    if (xs_.seen() < n_) {
      return NaN;
    }
    for (std::size_t k = 0; k < n_; ++k) {
      window_y_[static_cast<Eigen::Index>(k)] = ys_.ago(n_ - 1 - k);
      window_x_[static_cast<Eigen::Index>(k)] = xs_.ago(n_ - 1 - k);
    }
    const auto n = static_cast<Eigen::Index>(n_);
    Eigen::Matrix<double, Eigen::Dynamic, 2> design(n, 2);
    design << window_x_, Eigen::VectorXd::Ones(n);
    const Eigen::VectorXd beta = fit(window_y_, design);
    return Slope ? beta[0] : beta[1];
  }
  /**
   * @brief Least squares by the normal equations and LLT.
   * @param y The regressand.
   * @param x The design matrix.
   * @return The coefficients.
   */
  static Eigen::VectorXd fit(Eigen::Ref<const Eigen::VectorXd> y, Eigen::Ref<const Eigen::MatrixXd> x) {
    return (x.transpose() * x).llt().solve(x.transpose() * y);
  }
  /** @brief The regressands. */
  Ring ys_;
  /** @brief The regressors. */
  Ring xs_;
  /** @brief The window. */
  std::size_t n_{2};
  /** @brief The window's regressands, oldest first. */
  Eigen::VectorXd window_y_;
  /** @brief The window's regressors, oldest first. */
  Eigen::VectorXd window_x_;
};
#else
/**
 * @brief Without Eigen there is no `--indicators naive` least squares: a strategy that needs it does not compile.
 * @tparam Slope The slope if true, else the intercept.
 */
template <bool Slope>
struct OlsLlt {
  static_assert(
    Slope != Slope,
    "--indicators naive solves ols_slope and ols_intercept with Eigen: use an algotradelib built with ATL_USE_EIGEN=ON"
  );
};
#endif

/**
 * @brief A sliding-window sum with no subtraction ("two stacks").
 *
 * The window is an older part held as suffix sums and a newer part with a running total. Amortized O(1) per
 * value. Unlike a running sum, nothing cancels: a window of zeros sums to exactly 0, and a window of one
 * value is that value.
 */
struct WindowSum : Window {
  /**
   * @brief Sets the length and sizes the buffers.
   * @param n The length.
   */
  void set(std::size_t n) {
    Window::set(n);
    suffix_.assign(n + 1, 0.0);
    back_.assign(n + 1, 0.0);
  }
  /** @brief Restarts. */
  void reset() {
    begin_ = 0;
    end_ = 0;
    size_ = 0;
    back_sum_ = 0;
    seen_ = 0;
  }
  /**
   * @brief Adds the newest value.
   * @param x The value.
   * @return The sum of the last min(seen, n) values.
   */
  double push(double x) {
    const std::size_t n = this->n();
    back_[size_++] = x;
    back_sum_ += x;
    ++seen_;
    if (end_ - begin_ + size_ > n) {
      if (begin_ == end_) {
        // Move the newer part over as suffix sums, newest first.
        double sum = 0;
        for (std::size_t k = size_; k-- > 0;) {
          sum = back_[k] + sum;
          suffix_[k] = sum;
        }
        begin_ = 0;
        end_ = size_;
        size_ = 0;
        back_sum_ = 0;
      }
      ++begin_;
    }
    return (begin_ < end_ ? suffix_[begin_] : 0.0) + back_sum_;
  }
  /** @brief Values pushed since the last restart. */
  std::size_t seen() const { return seen_; }

private:
  std::vector<double> suffix_;
  std::vector<double> back_;
  std::size_t begin_{0};
  std::size_t end_{0};
  std::size_t size_{0};
  double back_sum_{0};
  std::size_t seen_{0};
};

/** @brief `Sma` over a subtraction-free window sum. Needs no ring. */
struct SmaExact : WindowSum {
  /**
   * @brief Takes the newest value.
   * @param x The value.
   * @return The mean, or NaN before n values.
   */
  double update(double x) {
    const double sum = this->push(x);
    return this->seen() >= this->n() ? sum / static_cast<double>(this->n()) : NaN;
  }
};

/** @brief `TsSum` over a subtraction-free window sum. Needs no ring. */
struct TsSumExact : WindowSum {
  /**
   * @brief Takes the newest value.
   * @param x The value.
   * @return The sum, or NaN before n values.
   */
  double update(double x) {
    const double sum = this->push(x);
    return this->seen() >= this->n() ? sum : NaN;
  }
};

/** @brief `Rsi` over two subtraction-free window sums, of up and of down moves. Needs no ring. */
struct RsiExact : Window {
  /**
   * @brief Sets the length.
   * @param n The length.
   */
  void set(std::size_t n) {
    Window::set(n);
    up_.set(n);
    down_.set(n);
  }
  /** @brief Restarts. */
  void reset() {
    up_.reset();
    down_.reset();
    seen_ = 0;
  }
  /**
   * @brief Takes the newest value.
   * @param x The value.
   * @return The RSI, or NaN before n + 1 values.
   */
  double update(double x) {
    ++seen_;
    if (seen_ >= 2) {
      const double d = x - last_;
      up_sum_ = up_.push(d > 0 ? std::fabs(d) : 0.0);
      down_sum_ = down_.push(d > 0 ? 0.0 : std::fabs(d));
    }
    last_ = x;
    if (seen_ < this->n() + 1) {
      return NaN;
    }
    return up_sum_ + down_sum_ == 0 ? 50 : 100 * up_sum_ / (up_sum_ + down_sum_);
  }

private:
  WindowSum up_;
  WindowSum down_;
  std::size_t seen_{0};
  double last_{0};
  double up_sum_{0};
  double down_sum_{0};
};

/** @brief EMA seeded with the mean of the first n values, alpha 2 / (n + 1). Needs no ring. */
struct Ema : Window {
  /** @brief Restarts. */
  void reset() {
    seen_ = 0;
    sum_ = 0;
    value_ = NaN;
  }
  /**
   * @brief Takes the newest value.
   * @param x The value.
   * @return The EMA, or NaN before n values.
   */
  double update(double x) {
    ATLS_NO_FMA
    const std::size_t n = this->n();
    ++seen_;
    if (seen_ < n) {
      sum_ += x;
      return NaN;
    }
    if (seen_ == n) {
      sum_ += x;
      value_ = sum_ / static_cast<double>(n);
    } else {
      const double alpha = 2.0 / (static_cast<double>(n) + 1.0);
      value_ = alpha * x + (1 - alpha) * value_;
    }
    return value_;
  }
  /** @brief Values taken since the last restart. */
  std::size_t seen_{0};
  /** @brief The sum of the first n values. */
  double sum_{0};
  /** @brief The last value. */
  double value_{NaN};
};

/**
 * @brief The highest (Max) or lowest value over the window. Incremental: a monotonic queue of its own;
 *   recompute: scans a ring of at least n.
 */
template <bool Max>
struct Extreme : Window {
  /**
   * @brief Sets the length and sizes the queue.
   * @param n The length.
   */
  void set(std::size_t n) {
    Window::set(n);
    values_.assign(n + 1, 0.0);
    stamps_.assign(n + 1, 0.0);
  }
  /** @brief Restarts. */
  void reset() {
    seen_ = 0;
    front_ = 0;
    size_ = 0;
  }
  /**
   * @brief Takes the newest value.
   * @param x The value.
   * @return The extreme, or NaN before n values.
   */
  double update(double x) {
    const std::size_t n = this->n();
    const std::size_t cap = values_.size();
    // Drop from the back every value the new one beats: it can never be the extreme again.
    while (size_ > 0) {
      const std::size_t back = (front_ + size_ - 1) % cap;
      if (Max ? values_[back] <= x : values_[back] >= x) {
        --size_;
      } else {
        break;
      }
    }
    const std::size_t at = (front_ + size_) % cap;
    values_[at] = x;
    stamps_[at] = static_cast<double>(seen_);
    ++size_;
    ++seen_;
    // Drop from the front what left the window.
    while (static_cast<double>(seen_) - stamps_[front_] > static_cast<double>(n)) {
      front_ = (front_ + 1) % cap;
      --size_;
    }
    return seen_ >= n ? values_[front_] : NaN;
  }
  /**
   * @brief Scans the window, oldest first.
   * @param ring The input's ring.
   * @return The extreme, or NaN before n values.
   */
  double recompute(const Ring& ring) const {
    const std::size_t n = this->n();
    if (ring.seen() < n) {
      return NaN;
    }
    double best = Max ? -std::numeric_limits<double>::infinity() : std::numeric_limits<double>::infinity();
    for (std::size_t k = n; k-- > 0;) {
      best = Max ? std::max(best, ring.ago(k)) : std::min(best, ring.ago(k));
    }
    return best;
  }

private:
  std::vector<double> values_;
  std::vector<double> stamps_;
  std::size_t seen_{0};
  std::size_t front_{0};
  std::size_t size_{0};
};

/** @brief Highest over the window. */
using TsMax = Extreme<true>;

/** @brief Lowest over the window. */
using TsMin = Extreme<false>;

/**
 * @brief What a handler states this bar: the last defined target, as whole shares truncated toward zero.
 *   An undefined, infinite or larger than 1e12 value states nothing.
 */
class Target {
public:
  /** @brief The largest number of shares a target may state. */
  static constexpr double MAX_SHARES = 1e12;

  /**
   * @brief States a real-valued target.
   * @param value The shares.
   */
  void state(double value) {
    if (std::isfinite(value) && std::fabs(value) <= MAX_SHARES) {
      stated_ = true;
      shares_ = static_cast<std::int64_t>(std::trunc(value));
    }
  }

  /**
   * @brief States a whole number of shares.
   * @param value The shares.
   */
  void state(std::int64_t value) {
    if (value <= static_cast<std::int64_t>(MAX_SHARES) && value >= -static_cast<std::int64_t>(MAX_SHARES)) {
      stated_ = true;
      shares_ = value;
    }
  }

  /** @brief Whether anything was stated. */
  bool stated() const { return stated_; }
  /** @brief The shares stated. */
  std::int64_t shares() const { return shares_; }

private:
  bool stated_{false};
  std::int64_t shares_{0};
};

/** @brief How many stated targets limit pricing did not submit, by reason. */
struct PricingRefusals {
  /** @brief The price of the event that ran the handler was missing or not positive. */
  std::size_t no_basis_price_{0};
  /** @brief The instrument had no reference data, or its price grid is unknown. */
  std::size_t no_price_grid_{0};
  /** @brief The limit's arithmetic, or its rounding onto the grid, did not fit a decimal. */
  std::size_t price_not_representable_{0};
  /** @brief The limit, rounded onto the grid, was not positive. */
  std::size_t price_not_positive_{0};
  /** @brief The net position, summed over every holding row, did not fit a decimal. */
  std::size_t net_not_representable_{0};

  /**
   * @brief Every refusal.
   * @return The sum of the counts.
   */
  std::size_t total() const {
    return no_basis_price_ + no_price_grid_ + price_not_representable_ + price_not_positive_ + net_not_representable_;
  }
};

/**
 * @brief How a stated target becomes an order: a market order (the default), or a limit priced from the price of the
 *   event that ran the handler.
 *
 * The limit is `basis × (1 + offset)` for a buy and `basis × (1 − offset)` for a sell, computed exactly, then put on
 * the instrument's tick grid on the safe side (`Price::rounded`: a buy down, a sell up). The side compares the target
 * with the net position summed over every holding row. A target equal to the position is submitted without a limit,
 * so that the runner still cancels unwanted working orders. A target that cannot be priced is not submitted and is
 * counted by reason (`refusals()`): a requested limit never becomes a market order.
 */
class OrderPricing {
public:
  /**
   * @brief Sets the pricing.
   * @param type `OrderType::MARKET` or `OrderType::LIMIT`.
   * @param offset The limit's offset, at least 0 and below 0.3; 0 with `MARKET`.
   * @throws std::invalid_argument For another type, an offset outside its range, an offset with `MARKET`, or an
   *   offset whose `1 ± offset` does not fit a decimal.
   */
  void set(atl::OrderType type, const atl::Decimal64& offset) {
    if (type != atl::OrderType::MARKET && type != atl::OrderType::LIMIT) {
      throw std::invalid_argument("order pricing: the order type is market or limit");
    }
    const atl::Decimal64 zero(0, 0);
    if (atl::decimal_less(offset, zero) || !atl::decimal_less(offset, atl::Decimal64(3, 1))) {
      throw std::invalid_argument("order pricing: the offset is at least 0 and below 0.3, not " + offset.str());
    }
    if (type == atl::OrderType::MARKET && offset.coefficient() != 0) {
      throw std::invalid_argument("order pricing: an offset prices a limit, not a market order");
    }
    const atl::Decimal64 one(1, 0);
    const auto buy = atl::decimal_add(one, offset);
    const auto sell = atl::decimal_subtract(one, offset);
    if (!buy || !sell) {
      throw std::invalid_argument("order pricing: 1 plus or minus the offset " + offset.str() + " does not fit a decimal");
    }
    limit_ = type == atl::OrderType::LIMIT;
    buy_ = *buy;
    sell_ = *sell;
  }

  /**
   * @brief Whether targets are priced as limits.
   * @return `true` after `set` with `OrderType::LIMIT`.
   */
  bool limit() const { return limit_; }

  /**
   * @brief Prices a target as a limit, reading the net position.
   * @param ctx The runner's context: its portfolio and the instrument's reference data.
   * @param order The target; its `limit_` is set when it is priced.
   * @param basis The price of the event that ran the handler.
   * @return Whether to submit the target; `false` after counting a refusal.
   */
  template <class Ctx>
  bool price(Ctx& ctx, atl::strategy::TargetPosition& order, const atl::Decimal64& basis) {
    const auto net = ctx.portfolio().net_quantity_of(order.instrument_);
    if (!net) {
      ++refusals_.net_not_representable_;
      return false;
    }
    return price(ctx, order, basis, *net);
  }

  /**
   * @brief Prices a target as a limit, against a net position the handler already read.
   * @param ctx The runner's context: the instrument's reference data.
   * @param order The target; its `limit_` is set when it is priced.
   * @param basis The price of the event that ran the handler.
   * @param net The net position, summed over every holding row.
   * @return Whether to submit the target; `false` after counting a refusal.
   */
  template <class Ctx>
  bool price(Ctx& ctx, atl::strategy::TargetPosition& order, const atl::Decimal64& basis, const atl::Decimal64& net) {
    const bool buy = atl::decimal_less(net, order.target_quantity_);
    if (!buy && !atl::decimal_less(order.target_quantity_, net)) {
      return true;
    }
    if (basis.coefficient() <= 0) {
      ++refusals_.no_basis_price_;
      return false;
    }
    const auto instrument = ctx.instrument(order.instrument_);
    if (!instrument || !instrument->price_rule_.grid_known()) {
      ++refusals_.no_price_grid_;
      return false;
    }
    const auto wanted = atl::decimal_multiply(basis, buy ? buy_ : sell_);
    if (!wanted) {
      ++refusals_.price_not_representable_;
      return false;
    }
    auto limit = atl::strategy::Price::rounded(*wanted, instrument->price_rule_, buy ? atl::OrderSide::BUY : atl::OrderSide::SELL);
    if (!limit) {
      ++refusals_.price_not_representable_;
      return false;
    }
    if (limit->value().coefficient() <= 0) {
      ++refusals_.price_not_positive_;
      return false;
    }
    order.limit_ = std::move(limit);
    return true;
  }

  /**
   * @brief The targets not submitted, by reason, since the strategy was made.
   * @return The counts.
   */
  const PricingRefusals& refusals() const { return refusals_; }

private:
  bool limit_{false};
  atl::Decimal64 buy_{1, 0};
  atl::Decimal64 sell_{1, 0};
  PricingRefusals refusals_;
};

} // namespace ops
} // namespace atls

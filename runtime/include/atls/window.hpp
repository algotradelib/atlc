#pragma once

#include "./expr.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <limits>
#include <type_traits>
#include <vector>

#if defined(ATL_HAS_EIGEN)
#include <Eigen/Dense>
#endif

/**
 * @file
 * @brief Rings and the built-in indicators over a window, in each arithmetic.
 *
 * A value without enough history is NaN; under `missing = restart` an undefined input restarts the window (the caller
 * clears the ring and calls `reset()`).
 */

namespace atls {
namespace ops {

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
  double ago(std::size_t k) const {
    return buffer_[head_ >= k ? head_ - k : head_ + capacity() - k];
  }

  /** @brief Values pushed since the last restart. */
  std::size_t seen() const {
    return seen_;
  }

  /** @brief How many values the ring holds. */
  std::size_t capacity() const {
    return buffer_.size();
  }

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
  void set(std::size_t n) {
    n_ = n;
  }
  /** @brief The length. */
  std::size_t n() const {
    return n_;
  }
  /** @brief The length. */
  std::size_t n_{1};
};

/** @brief Simple moving average. Incremental: a ring of at least n + 1; recompute: at least n. */
struct Sma : Window {
  /** @brief Restarts. */
  void reset() {
    sum_ = 0;
  }
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
  void reset() {
    sum_ = 0;
  }
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
  double update(const Ring& ring) const {
    return ring.seen() > this->n() ? ring.ago(this->n()) : NaN;
  }
  /**
   * @brief The same: a lag has nothing to recompute.
   * @param ring The input's ring.
   * @return As `update`.
   */
  double recompute(const Ring& ring) const {
    return update(ring);
  }
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
  void reset() {
    sd_.reset();
  }
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
  void set_min(std::size_t m) {
    min_ = m;
  }
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

/** @brief `ma(x, n, missing = skip)`: the mean of the values that exist in the last n slots. */
struct SmaSkip : SkipSums {
  /**
   * @brief After the ring took the newest slot.
   * @param ring The input's ring.
   * @return The mean, or NaN when the window is undefined.
   */
  double update(const Ring& ring) {
    return this->take(ring, false) ? this->sum_ / static_cast<double>(this->count_) : NaN;
  }
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

/** @brief `roll_sum(x, n, missing = skip)`: the sum of the values that exist in the last n slots. */
struct TsSumSkip : SkipSums {
  /**
   * @brief After the ring took the newest slot.
   * @param ring The input's ring.
   * @return The sum, or NaN when the window is undefined.
   */
  double update(const Ring& ring) {
    return this->take(ring, false) ? this->sum_ : NaN;
  }
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

/** @brief `roll_stdev(x, n, missing = skip)`: sample deviation over the values that exist. */
using StdevSkip = StdevSkipT<false>;

/** @brief `roll_stdev_biased(x, n, missing = skip)`: population deviation over the values that exist. */
using StdevPopSkip = StdevSkipT<true>;

/**
 * @brief `roll_zscore(x, n, missing = skip)`: the newest value against the values that exist; undefined if it is
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

/** @brief `roll_sum(x, n, m)`: the sum of the last n values, defined once m are in. Keeps its own ring. */
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
 * @brief `roll_ols(y, x, n)`, incremental and exact: least squares of y on x with an intercept over the last n bars,
 *   from running sums of x, y, x * x and x * y over its own rings; one instance gives the slope and the intercept.
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
   */
  void update(double y, double x) {
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
  }
  /** @brief The slope; undefined before n pairs and where the computed denominator is 0. */
  double slope() const {
    ATLS_NO_FMA
    if (xs_.seen() < n_) {
      return NaN;
    }
    const double n = static_cast<double>(n_);
    const double d = n * sxx_ - sx_ * sx_;
    return d != 0 ? (n * sxy_ - sx_ * sy_) / d : NaN;
  }
  /** @brief The intercept; undefined before n pairs and where the computed denominator is 0. */
  double intercept() const {
    ATLS_NO_FMA
    if (xs_.seen() < n_) {
      return NaN;
    }
    const double b = slope();
    return (sy_ - b * sx_) / static_cast<double>(n_);
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

#if defined(ATL_HAS_EIGEN)
/**
 * @brief `roll_ols(y, x, n)` with `--indicators naive`: the normal equations of [x, 1] solved again on every window
 *   by Eigen's LLT; one solve gives the slope and the intercept.
 * @tparam T Unused: a template, so that without Eigen only a strategy that uses it fails to compile.
 */
template <class T = void>
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
    slope_ = intercept_ = NaN;
  }
  /**
   * @brief Takes the newest pair, and solves once n pairs are in.
   * @param y The regressand.
   * @param x The regressor.
   */
  void update(double y, double x) {
    ys_.push(y);
    xs_.push(x);
    if (xs_.seen() < n_) {
      return;
    }
    for (std::size_t k = 0; k < n_; ++k) {
      window_y_[static_cast<Eigen::Index>(k)] = ys_.ago(n_ - 1 - k);
      window_x_[static_cast<Eigen::Index>(k)] = xs_.ago(n_ - 1 - k);
    }
    const auto n = static_cast<Eigen::Index>(n_);
    Eigen::Matrix<double, Eigen::Dynamic, 2> design(n, 2);
    design << window_x_, Eigen::VectorXd::Ones(n);
    const Eigen::VectorXd beta = fit(window_y_, design);
    slope_ = beta[0];
    intercept_ = beta[1];
  }
  /** @brief The slope, or NaN before n pairs. */
  double slope() const {
    return slope_;
  }
  /** @brief The intercept, or NaN before n pairs. */
  double intercept() const {
    return intercept_;
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
  /** @brief The last slope. */
  double slope_{NaN};
  /** @brief The last intercept. */
  double intercept_{NaN};
};
#else
/**
 * @brief Without Eigen there is no `--indicators naive` least squares: a strategy that needs it does not compile.
 * @tparam T Unused: the assertion depends on it, so it fires only where the class is used.
 */
template <class T = void>
struct OlsLlt {
  static_assert(
    !std::is_same<T, T>::value,
    "--indicators naive solves roll_ols with Eigen: use an algotradelib built with ATL_USE_EIGEN=ON"
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
  std::size_t seen() const {
    return seen_;
  }

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

} // namespace ops
} // namespace atls

#pragma once

#include "./ops.hpp"

#if !defined(ATL_HAS_EIGEN)
#error "atls/measures.hpp: a program with measures {} needs an algotradelib built with Eigen (ATL_USE_EIGEN=ON)"
#endif

#include <algotradelib/backtest>

#include <cmath>
#include <cstddef>

/**
 * @file
 * @brief What a program's `measures {}` calls: the points of a finished replay's equity curve, and the reductions
 *   over them.
 *
 * Every access to the library's `backtest::MeasureInput` is here. A reduction evaluates its argument at each point,
 * oldest first, and skips the undefined (NaN) values, as the library's own figures do: `sum` and `count` of nothing
 * are 0, the others NaN.
 */

namespace atls {

namespace measures {

/** @brief One point of the equity curve, as a measure reads it. */
struct Point {
  /** @brief The curve's level. */
  double equity_{ops::NaN};
  /** @brief The period return; NaN where the library leaves it missing. */
  double ret_{ops::NaN};
  /**
   * @brief `(peak - equity) / peak`, with `peak` the largest defined level so far; NaN where `peak` is at or below 0
   *   (the library's `max_drawdown` rule).
   */
  double drawdown_{ops::NaN};
};

/**
 * @brief Calls `f` with each point of the curve, oldest first.
 * @param in The finished replay.
 * @param f Called with each `Point`.
 */
template <class F>
void each(const atl::backtest::MeasureInput& in, F&& f) {
  ATLS_NO_FMA
  double peak = ops::NaN;
  for (atl::math::Index i = 0; i < in.levels_.size(); ++i) {
    Point p;
    p.equity_ = in.levels_[i];
    p.ret_ = i < in.returns_.size() ? in.returns_[i] : ops::NaN;
    if (!std::isnan(p.equity_) && (std::isnan(peak) || peak < p.equity_)) {
      peak = p.equity_;
    }
    if (!std::isnan(p.equity_) && peak > 0) {
      p.drawdown_ = (peak - p.equity_) / peak;
    }
    f(p);
  }
}

/**
 * @brief Calls `g` with each defined value of `f` over the curve, oldest first.
 * @param in The finished replay.
 * @param f The value at a point.
 * @param g Called with each value that is not NaN.
 */
template <class F, class G>
void each_defined(const atl::backtest::MeasureInput& in, F& f, G&& g) {
  each(in, [&](const Point& p) {
    const double v = f(p);
    if (!std::isnan(v)) {
      g(v);
    }
  });
}

/**
 * @brief The sum of the defined values.
 * @param in The finished replay.
 * @param f The value at a point.
 * @return The sum, oldest first; 0 over none.
 */
template <class F>
double sum(const atl::backtest::MeasureInput& in, F f) {
  double s = 0;
  each_defined(in, f, [&](double v) {
    s += v;
  });
  return s;
}

/**
 * @brief The mean of the defined values.
 * @param in The finished replay.
 * @param f The value at a point.
 * @return The mean; NaN over none.
 */
template <class F>
double mean(const atl::backtest::MeasureInput& in, F f) {
  double s = 0;
  std::size_t n = 0;
  each_defined(in, f, [&](double v) {
    s += v;
    ++n;
  });
  return n > 0 ? s / static_cast<double>(n) : ops::NaN;
}

/**
 * @brief The sample standard deviation of the defined values (n - 1 in the divisor), in two passes: the mean, then
 *   the squared deviations from it.
 * @param in The finished replay.
 * @param f The value at a point.
 * @return The deviation; NaN below two values; exactly 0 when every value equals the first (a mean that rounds away
 *   from them must not leave a tiny deviation, as the library's `sharpe` refuses a constant series).
 */
template <class F>
double stdev(const atl::backtest::MeasureInput& in, F f) {
  ATLS_NO_FMA
  std::size_t n = 0;
  double first = ops::NaN;
  bool constant = true;
  const double m = mean(in, [&](const Point& p) {
    const double v = f(p);
    if (!std::isnan(v)) {
      first = n == 0 ? v : first;
      constant = constant && v == first;
      ++n;
    }
    return v;
  });
  if (n < 2) {
    return ops::NaN;
  }
  if (constant) {
    return 0;
  }
  double squares = 0;
  each_defined(in, f, [&](double v) {
    ATLS_NO_FMA
    squares += (v - m) * (v - m);
  });
  return std::sqrt(squares / static_cast<double>(n - 1));
}

/**
 * @brief The defined value `keep` prefers to every other.
 * @param in The finished replay.
 * @param f The value at a point.
 * @param keep Whether a value replaces the one kept so far.
 * @return The value kept; NaN over none.
 */
template <class F, class Keep>
double pick(const atl::backtest::MeasureInput& in, F& f, Keep keep) {
  double out = ops::NaN;
  each_defined(in, f, [&](double v) {
    if (std::isnan(out) || keep(v, out)) {
      out = v;
    }
  });
  return out;
}

/** @brief The smallest defined value; NaN over none. @param in The finished replay. @param f The value at a point. */
template <class F>
double min(const atl::backtest::MeasureInput& in, F f) {
  return pick(in, f, [](double v, double kept) {
    return v < kept;
  });
}

/** @brief The largest defined value; NaN over none. @param in The finished replay. @param f The value at a point. */
template <class F>
double max(const atl::backtest::MeasureInput& in, F f) {
  return pick(in, f, [](double v, double kept) {
    return v > kept;
  });
}

/** @brief The first defined value; NaN over none. @param in The finished replay. @param f The value at a point. */
template <class F>
double first(const atl::backtest::MeasureInput& in, F f) {
  return pick(in, f, [](double, double) {
    return false;
  });
}

/** @brief The last defined value; NaN over none. @param in The finished replay. @param f The value at a point. */
template <class F>
double last(const atl::backtest::MeasureInput& in, F f) {
  return pick(in, f, [](double, double) {
    return true;
  });
}

/**
 * @brief How many points have a defined true value: not NaN and not 0.
 * @param in The finished replay.
 * @param f The value at a point (a three-valued `bool`: 1, 0 or NaN).
 * @return The count; 0 over none.
 */
template <class F>
double count(const atl::backtest::MeasureInput& in, F f) {
  std::size_t n = 0;
  each_defined(in, f, [&](double v) {
    n += v != 0 ? 1 : 0;
  });
  return static_cast<double>(n);
}

/**
 * @brief The run's periods per year.
 * @param in The finished replay.
 * @return As the run gave it; NaN when it gave none.
 */
inline double periods_per_year(const atl::backtest::MeasureInput& in) {
  return static_cast<double>(in.periods_per_year_);
}

/**
 * @brief The number of executions.
 * @param in The finished replay.
 * @return The count.
 */
inline double fills(const atl::backtest::MeasureInput& in) {
  return static_cast<double>(in.result_.executions_.size());
}

} // namespace measures
} // namespace atls

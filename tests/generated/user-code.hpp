// The C++ of the external indicators and functions in tests/generated/programs/identity-extern*.atls: the
// built-ins' arithmetic restated from runtime/include/atls/ops.hpp, one class per built-in and mode, in the
// runtime's order, so that a program can compare their values. Each throws if it is given an undefined value, which atls
// promises never to do. Floating-point contraction in user code is the user's: here it is off, as in the runtime.
#pragma once

#include <Eigen/Dense>

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <limits>
#include <stdexcept>
#include <vector>

#if defined(__clang__)
#define USER_NO_FMA _Pragma("clang fp contract(off)")
#else
#define USER_NO_FMA
#endif

namespace atls::user {

namespace detail {

/** @brief Throws for an undefined value. */
inline double defined(double x) {
  if (std::isnan(x)) {
    throw std::logic_error("user code was given an undefined value");
  }
  return x;
}

/** @brief The last values, oldest first: at most `n + 1` of them. */
struct Last {
  /**
   * @brief Adds the newest value.
   * @param x The value.
   */
  void push(double x) {
    xs_.push_back(defined(x));
    if (xs_.size() > n_ + 1) {
      xs_.pop_front();
    }
  }
  /** @brief Whether a value leaves the window of n with the newest push. */
  bool full() const {
    return xs_.size() > n_;
  }
  /** @brief The window's length. */
  std::size_t n_;
  /** @brief The values. */
  std::deque<double> xs_;
};

} // namespace detail

/** @brief `sma(x, n)` with `--indicators incremental`: a running sum. */
class inc_sma {
public:
  /** @brief A window of `n`. */
  explicit inc_sma(std::int64_t n) : last_{static_cast<std::size_t>(n), {}} {}
  /** @brief Takes the newest value. */
  void update(double x) {
    last_.push(x);
    sum_ += last_.xs_.back();
    if (last_.full()) {
      sum_ -= last_.xs_.front();
    }
  }
  /** @brief The mean. */
  double mean() const {
    return sum_ / static_cast<double>(last_.n_);
  }

private:
  detail::Last last_;
  double sum_{0};
};

/** @brief `sma(x, n)` with `--indicators naive`: the window summed again, oldest first. */
class naive_sma {
public:
  /** @brief A window of `n`. */
  explicit naive_sma(std::int64_t n) : last_{static_cast<std::size_t>(n), {}} {}
  /** @brief Takes the newest value. */
  void update(double x) {
    last_.push(x);
  }
  /** @brief The mean. */
  double mean() const {
    double sum = 0;
    for (std::size_t k = last_.full() ? 1 : 0; k < last_.xs_.size(); ++k) {
      sum += last_.xs_[k];
    }
    return sum / static_cast<double>(last_.n_);
  }

private:
  detail::Last last_;
};

/** @brief `sma(x, n)` with `--indicators exact`: a sliding sum without subtraction ("two stacks"). */
class exact_sma {
public:
  /** @brief A window of `n`. */
  explicit exact_sma(std::int64_t n) : n_(static_cast<std::size_t>(n)), suffix_(n_ + 1), back_(n_ + 1) {}
  /** @brief Takes the newest value. */
  void update(double x) {
    back_[size_++] = detail::defined(x);
    back_sum_ += x;
    if (end_ - begin_ + size_ > n_) {
      if (begin_ == end_) {
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
    sum_ = (begin_ < end_ ? suffix_[begin_] : 0.0) + back_sum_;
  }
  /** @brief The mean. */
  double mean() const {
    return sum_ / static_cast<double>(n_);
  }

private:
  std::size_t n_;
  std::vector<double> suffix_;
  std::vector<double> back_;
  std::size_t begin_{0};
  std::size_t end_{0};
  std::size_t size_{0};
  double back_sum_{0};
  double sum_{0};
};

/** @brief `ols_slope(y, x, n)` and `ols_intercept(y, x, n)` with `--indicators incremental`: running sums. */
class inc_ols {
public:
  /** @brief A window of `n` pairs. */
  explicit inc_ols(std::int64_t n) : ys_{static_cast<std::size_t>(n), {}}, xs_{static_cast<std::size_t>(n), {}} {}
  /** @brief Takes the newest pair. */
  void update(double y, double x) {
    USER_NO_FMA
    ys_.push(y);
    xs_.push(x);
    sx_ += x;
    sy_ += y;
    sxx_ += x * x;
    sxy_ += x * y;
    if (xs_.full()) {
      const double oy = ys_.xs_.front();
      const double ox = xs_.xs_.front();
      sx_ -= ox;
      sy_ -= oy;
      sxx_ -= ox * ox;
      sxy_ -= ox * oy;
    }
  }
  /** @brief The slope; undefined for a flat x. */
  double slope() const {
    USER_NO_FMA
    const double n = static_cast<double>(xs_.n_);
    const double d = n * sxx_ - sx_ * sx_;
    return d != 0 ? (n * sxy_ - sx_ * sy_) / d : std::numeric_limits<double>::quiet_NaN();
  }
  /** @brief The intercept. */
  double intercept() const {
    USER_NO_FMA
    const double b = slope();
    return (sy_ - b * sx_) / static_cast<double>(xs_.n_);
  }

private:
  detail::Last ys_;
  detail::Last xs_;
  double sx_{0};
  double sy_{0};
  double sxx_{0};
  double sxy_{0};
};

/** @brief `ols_slope` and `ols_intercept` with `--indicators exact`: running sums, as incremental. */
class exact_ols : public inc_ols {
public:
  using inc_ols::inc_ols;
};

/** @brief `ols_slope` and `ols_intercept` with `--indicators naive`: the normal equations of [x, 1] by Eigen's LLT. */
class naive_ols {
public:
  /** @brief A window of `n` pairs. */
  explicit naive_ols(std::int64_t n) : ys_{static_cast<std::size_t>(n), {}}, xs_{static_cast<std::size_t>(n), {}} {}
  /** @brief Takes the newest pair. */
  void update(double y, double x) {
    ys_.push(y);
    xs_.push(x);
    const auto n = static_cast<Eigen::Index>(xs_.n_);
    if (static_cast<Eigen::Index>(xs_.xs_.size()) < n) {
      return;
    }
    Eigen::VectorXd wy(n);
    Eigen::VectorXd wx(n);
    const std::size_t first = xs_.full() ? 1 : 0;
    for (Eigen::Index k = 0; k < n; ++k) {
      wy[k] = ys_.xs_[first + static_cast<std::size_t>(k)];
      wx[k] = xs_.xs_[first + static_cast<std::size_t>(k)];
    }
    Eigen::Matrix<double, Eigen::Dynamic, 2> design(n, 2);
    design << wx, Eigen::VectorXd::Ones(n);
    const Eigen::MatrixXd d = design;
    beta_ = (d.transpose() * d).llt().solve(d.transpose() * wy);
  }
  /** @brief The slope. */
  double slope() const {
    return beta_[0];
  }
  /** @brief The intercept. */
  double intercept() const {
    return beta_[1];
  }

private:
  detail::Last ys_;
  detail::Last xs_;
  Eigen::VectorXd beta_{Eigen::VectorXd::Zero(2)};
};

/** @brief `min(max(z, -cap), cap)`. */
inline double clip_c(double z, double cap) {
  return std::min(std::max(detail::defined(z), -detail::defined(cap)), cap);
}

/** @brief `z > -cap and z < cap`. */
inline bool inside_c(double z, double cap) {
  return detail::defined(z) > -detail::defined(cap) && z < cap;
}

/** @brief `2 * k + 1`. */
inline std::int64_t odd_c(std::int64_t k) {
  return 2 * k + 1;
}

// Last: from here on, `std` inside `atls::user` names this class, so the code below writes `::std`.

/** @brief The last value, for tests/generated/programs/extern-std.atls: a class named `std`. */
class std {
public:
  /** @brief Takes the newest value. */
  void update(double x) {
    last_ = detail::defined(x);
  }
  /** @brief The last value. */
  double value() const {
    return last_;
  }

private:
  double last_{0};
};

/** @brief `n + 1`, declared beside the class `std`. */
inline ::std::int64_t number(::std::int64_t n) {
  return n + 1;
}

} // namespace atls::user

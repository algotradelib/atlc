#pragma once

#include <algotradelib/strategy>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <string>
#include <vector>

/**
 * @file
 * @brief Strategies written by hand the way an experienced user of the library would (running sums over a
 *   ring, no bar history), to compare compiled programs with: `programs/pullback.atls` and
 *   `programs/trail.atls`.
 */

namespace hand {

namespace atl = algotradelib;

/**
 * @brief A decimal as a double, as the runtime reads prices.
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

/** @brief A ring of the last `capacity` closes. */
struct Closes {
  /** @brief The values. */
  std::vector<double> buffer_;
  /** @brief Where the newest is. */
  std::size_t head_{0};
  /** @brief Values pushed. */
  std::size_t seen_{0};

  /**
   * @brief An empty ring.
   * @param capacity How many values it holds.
   */
  explicit Closes(std::size_t capacity) : buffer_(capacity, 0.0) {}

  /**
   * @brief Adds the newest value.
   * @param x The value.
   */
  void push(double x) {
    head_ = head_ + 1 == buffer_.size() ? 0 : head_ + 1;
    buffer_[head_] = x;
    ++seen_;
  }

  /**
   * @brief The value `k` pushes ago.
   * @param k Pushes ago.
   * @return The value.
   */
  double ago(std::size_t k) const { return buffer_[head_ >= k ? head_ - k : head_ + buffer_.size() - k]; }
};

/**
 * @brief An RSI pullback above the trend average: the average and Cutler's RSI kept as running sums.
 *   `programs/pullback.atls` by hand.
 */
struct Pullback {
  /** @brief Its own lookback: no bar history. */
  static constexpr bool reads_bar_history = false;

  /** @brief The parameters. */
  struct Params {
    /** @brief The trend average's window. */
    std::size_t trend_{100};
    /** @brief The RSI's moves. */
    std::size_t fast_{3};
    /** @brief Buy below this RSI. */
    double entry_{20};
    /** @brief Sell above this RSI. */
    double exit_{70};
    /** @brief The shares bought. */
    std::int64_t quantity_{100};
  };

  /**
   * @brief Trades `instrument`.
   * @param instrument The instrument.
   * @param params The parameters.
   */
  Pullback(atl::InstrumentId instrument, Params params)
  : instrument_(std::move(instrument)), params_(params), closes_(std::max(params.trend_ + 1, params.fast_ + 2)) {}

  /**
   * @brief Updates the sums and states a target.
   * @param ctx The runner's context.
   * @param bar The bar.
   */
  template <class Ctx>
  void on_bar(Ctx& ctx, const atl::strategy::BarEvent& bar) {
    if (!(bar.instrument_ == instrument_)) {
      return;
    }
    const double close = to_double(bar.bar_.close_);
    closes_.push(close);
    const std::size_t seen = closes_.seen_;
    sum_ += close;
    if (seen > params_.trend_) {
      sum_ -= closes_.ago(params_.trend_);
    }
    if (seen >= 2) {
      const double d = close - closes_.ago(1);
      (d > 0 ? up_ : down_) += std::fabs(d);
    }
    if (seen >= params_.fast_ + 2) {
      const double d = closes_.ago(params_.fast_) - closes_.ago(params_.fast_ + 1);
      (d > 0 ? up_ : down_) -= std::fabs(d);
    }
    if (seen <= params_.trend_ || seen < params_.fast_ + 1) {
      return;
    }
    const double average = sum_ / static_cast<double>(params_.trend_);
    const double level = up_ + down_ == 0 ? 50 : 100 * up_ / (up_ + down_);
    const auto held = ctx.portfolio().net_quantity_of(instrument_);
    if (!held) {
      return;
    }
    const bool holding = held->coefficient() != 0;
    std::int64_t target = 0;
    if (!holding && close > average && level < params_.entry_) {
      target = params_.quantity_;
    } else if (holding && (level > params_.exit_ || close < average)) {
      target = 0;
    } else {
      return;
    }
    atl::strategy::TargetPosition order;
    order.instrument_ = instrument_;
    order.tag_ = "pullback";
    order.target_quantity_ = atl::Decimal64(target, 0);
    ctx.submit(order);
  }

  /** @brief The instrument. */
  atl::InstrumentId instrument_;
  /** @brief The parameters. */
  Params params_;
  /** @brief The closes. */
  Closes closes_;
  /** @brief The trend window's sum. */
  double sum_{0};
  /** @brief The sum of up moves. */
  double up_{0};
  /** @brief The sum of down moves. */
  double down_{0};
};

/**
 * @brief A moving-average trend entry with a trailing stop, a time stop and a cooldown.
 *   `programs/trail.atls` by hand.
 */
struct TrailingTrend {
  /** @brief Its own lookback: no bar history. */
  static constexpr bool reads_bar_history = false;

  /** @brief The parameters. */
  struct Params {
    /** @brief The fast average's window. */
    std::size_t fast_{20};
    /** @brief The slow average's window. */
    std::size_t slow_{60};
    /** @brief The trailing stop, as a fraction below the peak. */
    double trail_{0.05};
    /** @brief The time stop, in bars. */
    std::int64_t hold_{120};
    /** @brief Bars to wait after an exit. */
    std::int64_t cooldown_{30};
    /** @brief The shares bought. */
    std::int64_t quantity_{100};
  };

  /**
   * @brief Trades `instrument`.
   * @param instrument The instrument.
   * @param params The parameters.
   */
  TrailingTrend(atl::InstrumentId instrument, Params params)
  : instrument_(std::move(instrument)), params_(params), closes_(params.slow_ + 1), since_exit_(params.cooldown_) {}

  /**
   * @brief Starts or ends a trade.
   * @param fill The fill.
   */
  template <class Ctx>
  void on_fill(Ctx&, const atl::strategy::Fill& fill) {
    if (!(fill.instrument_ == instrument_)) {
      return;
    }
    if (fill.side_ == atl::OrderSide::BUY) {
      in_trade_ = true;
      peak_ = to_double(fill.price_);
      bars_in_ = 0;
    } else {
      in_trade_ = false;
      since_exit_ = 0;
    }
  }

  /**
   * @brief Updates the averages and states a target.
   * @param ctx The runner's context.
   * @param bar The bar.
   */
  template <class Ctx>
  void on_bar(Ctx& ctx, const atl::strategy::BarEvent& bar) {
    if (!(bar.instrument_ == instrument_)) {
      return;
    }
    const double close = to_double(bar.bar_.close_);
    closes_.push(close);
    const std::size_t seen = closes_.seen_;
    fast_sum_ += close;
    slow_sum_ += close;
    if (seen > params_.fast_) {
      fast_sum_ -= closes_.ago(params_.fast_);
    }
    if (seen > params_.slow_) {
      slow_sum_ -= closes_.ago(params_.slow_);
    }
    if (seen < params_.slow_) {
      return;
    }
    const double fast_avg = fast_sum_ / static_cast<double>(params_.fast_);
    const double slow_avg = slow_sum_ / static_cast<double>(params_.slow_);
    const auto held = ctx.portfolio().net_quantity_of(instrument_);
    if (!held) {
      return;
    }
    bool stated = false;
    std::int64_t target = 0;
    if (in_trade_) {
      ++bars_in_;
      peak_ = std::max(peak_, close);
      if (close < peak_ * (1 - params_.trail_) || bars_in_ >= params_.hold_ || fast_avg < slow_avg) {
        stated = true;
        target = 0;
      }
    } else {
      ++since_exit_;
      if (fast_avg > slow_avg && held->coefficient() == 0 && since_exit_ > params_.cooldown_) {
        stated = true;
        target = params_.quantity_;
      }
    }
    if (!stated) {
      return;
    }
    atl::strategy::TargetPosition order;
    order.instrument_ = instrument_;
    order.tag_ = "trail";
    order.target_quantity_ = atl::Decimal64(target, 0);
    ctx.submit(order);
  }

  /** @brief The instrument. */
  atl::InstrumentId instrument_;
  /** @brief The parameters. */
  Params params_;
  /** @brief The closes. */
  Closes closes_;
  /** @brief The fast window's sum. */
  double fast_sum_{0};
  /** @brief The slow window's sum. */
  double slow_sum_{0};
  /** @brief Whether a position is open. */
  bool in_trade_{false};
  /** @brief The highest close since the buy. */
  double peak_{0};
  /** @brief Bars since the buy. */
  std::int64_t bars_in_{0};
  /** @brief Bars since the last exit. */
  std::int64_t since_exit_;
};

} // namespace hand

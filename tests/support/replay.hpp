#pragma once

#include <algotradelib/backtest>
#include <algotradelib/strategy>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <random>
#include <string>
#include <utility>
#include <vector>

// The KRX stock tick table moved out of the library's core into <algotradelib/krx>; this builds with either library.
// Drop the old branch when that library is published (plan/todo/krx-include-cleanup.md).
#if __has_include(<algotradelib/krx>)
#include <algotradelib/krx>
#endif

/**
 * @file
 * @brief What the C++ tests share: deterministic bars, the replay settings, and a run's fills and report as
 *   one comparable text.
 */

namespace atlc_test {

namespace atl = algotradelib;

/**
 * @brief The KRX stock tick table.
 * @return The library's rule for KRX stocks.
 */
inline atl::PriceRule krx_equity_rule() {
#if __has_include(<algotradelib/krx>)
  return atl::krx::equity_price_rule();
#else
  return atl::PriceRule::krx_equity();
#endif
}

/** @brief One instrument's daily bars. */
struct Series {
  /** @brief The symbol. */
  std::string symbol_;
  /** @brief The bars, oldest first; a day may be missing. */
  std::vector<atl::Bar> bars_;
};

/**
 * @brief An instrument on the Korean exchange, quoted on its tick table.
 * @param symbol The symbol.
 * @return Its id.
 */
inline atl::InstrumentId id_of(const std::string& symbol) {
  return atl::InstrumentId{atl::Country::KR, atl::Venue::COMBINED, symbol};
}

/**
 * @brief A seeded random walk of daily bars on the tick grid of 10 between 5,000 and 19,990.
 *
 * Only `std::mt19937`'s output is used, which the standard fixes, so every compiler draws the same bars.
 * Whole-number prices keep running sums exact, as a hand-written strategy's are.
 *
 * @param symbol The symbol.
 * @param seed The seed.
 * @param days How many days.
 * @param missing One day in `missing` is left out (0: none).
 * @param listed The first day with a bar.
 * @return The series.
 */
inline Series walk(
  const std::string& symbol, unsigned seed, std::size_t days, unsigned missing = 0, std::size_t listed = 0
) {
  std::mt19937 rng(seed);
  const auto draw = [&rng](std::int64_t spread) {
    return static_cast<std::int64_t>(rng() % static_cast<std::uint32_t>(2 * spread + 1)) - spread;
  };
  const auto clamp = [](std::int64_t price) {
    return std::min<std::int64_t>(std::max<std::int64_t>(price, 500), 1999);
  };
  const auto decimal = [](std::int64_t tens) {
    return atl::Decimal64(tens * 10, 0);
  };
  // 2020-01-01 00:00 UTC.
  const auto first = std::chrono::system_clock::time_point{} + std::chrono::hours(24 * 18262);
  Series out;
  out.symbol_ = symbol;
  std::int64_t close = 1000 + draw(300);
  for (std::size_t day = 0; day < days; ++day) {
    const std::int64_t open = clamp(close + draw(3));
    close = clamp(close + draw(10));
    const std::int64_t high = clamp(std::max(open, close) + draw(2) + 2);
    const std::int64_t low = clamp(std::min(open, close) - draw(2) - 2);
    const bool skipped = missing != 0 && rng() % missing == 0;
    if (day < listed || skipped) {
      continue;
    }
    atl::Bar bar;
    bar.timestamp_ = first + std::chrono::hours(24 * static_cast<std::int64_t>(day));
    bar.open_ = decimal(open);
    bar.high_ = decimal(high);
    bar.low_ = decimal(low);
    bar.close_ = decimal(close);
    bar.volume_ = atl::Decimal64(100000, 0);
    out.bars_.push_back(bar);
  }
  return out;
}

/**
 * @brief Every bar of every series as an event, in time order (then series order): one period's bars
 *   together, as a strategy with `on_bars` needs them.
 * @param all The series.
 * @return The events.
 */
inline std::vector<atl::strategy::BarEvent> daily_events(const std::vector<Series>& all) {
  std::vector<std::pair<std::size_t, std::size_t>> order;
  for (std::size_t k = 0; k < all.size(); ++k) {
    for (std::size_t i = 0; i < all[k].bars_.size(); ++i) {
      order.emplace_back(k, i);
    }
  }
  std::stable_sort(order.begin(), order.end(), [&all](const auto& a, const auto& b) {
    return all[a.first].bars_[a.second].timestamp_ < all[b.first].bars_[b.second].timestamp_;
  });
  std::vector<atl::strategy::BarEvent> out;
  for (const auto& [k, i] : order) {
    out.push_back(
      atl::strategy::make_bar_event(
        id_of(all[k].symbol_), atl::BarInterval::ONE_DAY, atl::BarStamp::OPEN, all[k].bars_[i]
      )
    );
  }
  return out;
}

/**
 * @brief Each daily bar (stamped at a UTC midnight) as two close-stamped daily bars on the KRX session: a point at
 *   the open (09:00 KST, its open alone) and the day's bar at the close (15:30 KST). A program's `on_open` runs on
 *   the first, its `on_close` on the second.
 * @param all The series.
 * @return The events, in time order.
 */
inline std::vector<atl::strategy::BarEvent> session_events(const std::vector<Series>& all) {
  std::vector<atl::strategy::BarEvent> out;
  const auto session = atl::strategy::SessionWindow::krx();
  const auto days = daily_events(all);
  for (const auto& day : days) {
    const auto noon = day.bar_.timestamp_ + std::chrono::hours(3);
    atl::Bar point = day.bar_;
    point.timestamp_ = session.opens_at(noon);
    point.high_ = point.low_ = point.close_ = *point.open_;
    out.push_back(
      atl::strategy::make_bar_event(day.instrument_, atl::BarInterval::ONE_DAY, atl::BarStamp::CLOSE, point)
    );
    atl::Bar whole = day.bar_;
    whole.timestamp_ = session.closes_at(noon);
    out.push_back(
      atl::strategy::make_bar_event(day.instrument_, atl::BarInterval::ONE_DAY, atl::BarStamp::CLOSE, whole)
    );
  }
  // Every instrument's open before any day's bar.
  std::stable_sort(out.begin(), out.end(), [](const auto& a, const auto& b) {
    return a.bar_.timestamp_ < b.bar_.timestamp_;
  });
  return out;
}

/** @brief Costs, cash and shorts of a replay. */
struct Settings {
  /** @brief The opening cash. */
  std::string cash_{"1000000000"};
  /** @brief The commission rate. */
  std::string commission_{"0.00015"};
  /** @brief The tax rate on sells. */
  std::string sell_tax_{"0.0020"};
  /** @brief Slippage in ticks. */
  int slippage_ticks_{1};
  /** @brief Whether a position may go below zero. */
  bool shorts_{false};
  /** @brief Whether a market order fills at the close it was decided on (a market-price rule) instead of the open. */
  bool decision_close_{false};
};

/** @brief A replay's result and report. */
struct Outcome {
  /** @brief The result. */
  atl::backtest::ReplayResult result_;
  /** @brief The report. */
  atl::backtest::Report report_;
};

/**
 * @brief Replays `strategy` over `events`.
 * @param strategy The strategy.
 * @param symbols Every symbol traded.
 * @param events The bars, in time order.
 * @param settings The costs.
 * @return The result and the report.
 */
template <class S>
Outcome run(
  S strategy, const std::vector<std::string>& symbols, const std::vector<atl::strategy::BarEvent>& events,
  const Settings& settings = {}
) {
  atl::backtest::ReplayConfig config;
  config.cash_ = atl::Decimal64::parse(settings.cash_);
  for (const auto& symbol : symbols) {
    atl::Instrument instrument;
    instrument.id_ = id_of(symbol);
    instrument.name_ = symbol;
    instrument.type_ = atl::SecurityType::STOCK;
    instrument.currency_ = atl::Currency{"KRW"};
    instrument.lot_size_ = atl::Decimal64(1, 0);
    instrument.price_rule_ = krx_equity_rule();
    config.instruments_.push_back(instrument);
  }
  config.limits_.permitted_phases_ = std::nullopt;
  config.limits_.enforce_sellable_quantity_ = !settings.shorts_;
  auto fills =
    atl::backtest::NextBarFillModel(
      atl::backtest::CommissionModel::of(atl::Decimal64::parse(settings.commission_), atl::Decimal64(0, 0)),
      atl::backtest::SlippageModel::of_ticks(settings.slippage_ticks_)
    )
      .with_sell_tax(atl::backtest::SellTax{atl::Decimal64::parse(settings.sell_tax_), atl::Country::KR});
  if (settings.decision_close_) {
    fills = fills.with_market_price("decision-close", [](const atl::backtest::MarketPriceInput& in) {
      return in.decision_price_;
    });
  }
  atl::backtest::Backtester<S> engine(std::move(strategy), config, fills);
  for (const auto& event : events) {
    engine.feed(event);
  }
  engine.finish();
  Outcome outcome;
  outcome.result_ = engine.result();
  outcome.report_ = atl::backtest::summarize(outcome.result_, engine.fills(), 252.0);
  return outcome;
}

/**
 * @brief A number with every bit: equal texts are equal doubles (or both undefined).
 * @param x The number.
 * @return Its text.
 */
inline std::string bits(double x) {
  if (std::isnan(x)) {
    return "nan";
  }
  char buffer[40];
  std::snprintf(buffer, sizeof buffer, "%.17g", x);
  return buffer;
}

/**
 * @brief Every fill (time, symbol, side, quantity, price, costs; not the tag) and the report, as text.
 * @param outcome A replay.
 * @return The text.
 */
inline std::string signature(const Outcome& outcome) {
  std::string out;
  for (const auto& e : outcome.result_.executions_) {
    const auto at = std::chrono::duration_cast<std::chrono::minutes>(e.fill_.at_.time_since_epoch()).count();
    out += std::to_string(at) + ' ' + e.fill_.instrument_.symbol_ +
      (e.fill_.side_ == atl::OrderSide::BUY ? " buy " : " sell ") + e.fill_.quantity_.str() + ' ' +
      e.fill_.price_.str() + ' ' + e.commission_.str() + ' ' + e.slippage_.str() + ' ' + e.tax_.str() + '\n';
  }
  const auto& r = outcome.report_;
  const auto optional = [](const std::optional<atl::Decimal64>& d) {
    return d ? d->str() : std::string("none");
  };
  out += "bars " + std::to_string(r.bars_) + " executions " + std::to_string(r.executions_) + " equity " +
    r.opening_equity_.str() + ' ' + r.closing_equity_.str() + " return " + bits(r.total_return_) + " sharpe " +
    bits(r.sharpe_) + ' ' + bits(r.annualized_sharpe_) + " sortino " + bits(r.sortino_) + " drawdown " +
    bits(r.max_drawdown_) + " turnover " + bits(r.total_turnover_) + " commission " + optional(r.commission_paid_) +
    " slippage " + optional(r.slippage_paid_) + " tax " + optional(r.tax_paid_) + " rejected " +
    std::to_string(r.rejected_) + " refused " + std::to_string(r.refused_) + " aborted " + std::to_string(r.aborted_) +
    '\n';
  return out;
}

/**
 * @brief A compiled strategy with parameters set by name, in order (each `set` checks the bounds again).
 * @param strategy A compiled strategy.
 * @param params Names and values.
 * @return The strategy with the values set.
 */
template <class S>
S with(S strategy, const std::vector<std::pair<std::string, double>>& params) {
  for (const auto& [name, value] : params) {
    strategy.set(name, value);
  }
  return strategy;
}

} // namespace atlc_test

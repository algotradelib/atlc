// Measures of compiled programs over replayed runs: each equals the figure computed here from the result's equity
// curve; inside `measures {}`, `ret` is the curve's period return, not the program's series of that name; a universe
// program states the same functions; and a chain of measures walks the curve once per reduction.

#include <cstddef>

namespace atls::user {

/** How many times `counted` ran. */
inline std::size_t counted_calls = 0;

/** Its argument; counts the call. */
inline double counted(double x) {
  ++counted_calls;
  return x;
}

} // namespace atls::user

#include "replay.hpp"

#include <measures_extern.hpp>
#include <measures_single.hpp>
#include <measures_universe.hpp>

#include <algotradelib/analytics>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_floating_point.hpp>

#include <cmath>
#include <vector>

namespace {

namespace atl = algotradelib;
namespace t = atlc_test;
using Catch::Matchers::WithinRel;

ATL_ASSERT_STRATEGY(atls::MeasuresSingle, atl::strategy::MinimalContext);
ATL_ASSERT_STRATEGY(atls::MeasuresUniverse, atl::strategy::MinimalContext);

/** The figures of a curve, computed point by point. */
struct ByHand {
  double mean_{0};
  double var_{0};
  double total_{0};
  double max_dd_{0};
  double under_{0};
  double lowest_{0};
  std::size_t defined_{0};
};

ByHand by_hand(const atl::backtest::ReplayResult& result) {
  ATLS_NO_FMA
  ByHand out;
  double sum = 0;
  for (const auto& p : result.equity_) {
    if (!std::isnan(p.period_return_)) {
      sum += p.period_return_;
      ++out.defined_;
    }
  }
  out.mean_ = sum / static_cast<double>(out.defined_);
  double squares = 0;
  for (const auto& p : result.equity_) {
    if (!std::isnan(p.period_return_)) {
      squares += (p.period_return_ - out.mean_) * (p.period_return_ - out.mean_);
    }
  }
  out.var_ = squares / static_cast<double>(out.defined_ - 1);
  double peak = 0;
  out.lowest_ = atl::analytics::to_scalar(result.equity_.front().equity_);
  for (const auto& p : result.equity_) {
    const double level = atl::analytics::to_scalar(p.equity_);
    peak = std::max(peak, level);
    out.max_dd_ = std::max(out.max_dd_, (peak - level) / peak);
    out.under_ += (peak - level) / peak > 0.01 ? 1 : 0;
    out.lowest_ = std::min(out.lowest_, level);
  }
  const double first = atl::analytics::to_scalar(result.equity_.front().equity_);
  out.total_ = atl::analytics::to_scalar(result.equity_.back().equity_) / first - 1;
  return out;
}

} // namespace

TEST_CASE("each measure equals the figure computed from the curve") {
  const auto series = t::walk("A", 11, 400, 9);
  const auto outcome = t::run(atls::MeasuresSingle(t::id_of("A")), {"A"}, t::daily_events({series}));
  const auto in = atl::backtest::measure_input(outcome.result_, atl::backtest::NextBarFillModel{}, 252.0);
  REQUIRE(outcome.result_.equity_.size() > 100);
  REQUIRE(!outcome.result_.executions_.empty());
  const ByHand h = by_hand(outcome.result_);
  using S = atls::MeasuresSingle;
  CHECK_THAT(S::measure_mean_ret(in), WithinRel(h.mean_, 1e-12));
  CHECK_THAT(S::measure_var_ret(in), WithinRel(h.var_, 1e-12));
  CHECK_THAT(S::measure_sd(in), WithinRel(std::sqrt(h.var_), 1e-12));
  CHECK_THAT(S::measure_sharpe(in), WithinRel(h.mean_ / std::sqrt(h.var_) * std::sqrt(252.0), 1e-12));
  CHECK_THAT(S::measure_total(in), WithinRel(h.total_, 1e-12));
  CHECK_THAT(S::measure_max_dd(in), WithinRel(h.max_dd_, 1e-12));
  CHECK(S::measure_max_dd(in) == atl::analytics::max_drawdown(in.levels_).magnitude_);
  CHECK(S::measure_under(in) == h.under_);
  CHECK(S::measure_points(in) == static_cast<double>(outcome.result_.equity_.size()));
  CHECK(S::measure_trades(in) == static_cast<double>(outcome.result_.executions_.size()));
  CHECK(S::measure_one(in) == 1);
  CHECK(S::measure_lowest(in) == h.lowest_);
  CHECK(S::measure_flat_sd(in) == 0);
  CHECK(std::isnan(S::measure_flat_ratio(in)));
  // The program's series `ret` is the close-to-close return, whose mean is not the curve's.
  double closes = 0;
  for (std::size_t i = 1; i < series.bars_.size(); ++i) {
    closes +=
      atl::analytics::to_scalar(series.bars_[i].close_) / atl::analytics::to_scalar(series.bars_[i - 1].close_) - 1;
  }
  CHECK(std::abs(S::measure_mean_ret(in) - closes / static_cast<double>(series.bars_.size() - 1)) > 1e-6);
  // Without periods per year, the measures that read it are undefined.
  const auto bare = atl::backtest::measure_input(outcome.result_, atl::backtest::NextBarFillModel{});
  CHECK(std::isnan(S::measure_sharpe(bare)));
}

TEST_CASE("a universe program states the same measures") {
  const std::vector<t::Series> all{t::walk("A", 3, 300, 7), t::walk("B", 4, 300, 7), t::walk("C", 5, 300)};
  const std::vector<atl::InstrumentId> ids{t::id_of("A"), t::id_of("B"), t::id_of("C")};
  const auto outcome = t::run(atls::MeasuresUniverse(ids), {"A", "B", "C"}, t::daily_events(all));
  const auto in = atl::backtest::measure_input(outcome.result_, atl::backtest::NextBarFillModel{}, 252.0);
  REQUIRE(!outcome.result_.executions_.empty());
  const ByHand h = by_hand(outcome.result_);
  CHECK_THAT(atls::MeasuresUniverse::measure_mean_ret(in), WithinRel(h.mean_, 1e-12));
  CHECK(atls::MeasuresUniverse::measure_points(in) == static_cast<double>(outcome.result_.equity_.size()));
}

TEST_CASE("a chain of measures calls the first reduction's function once per defined return") {
  const auto outcome = t::run(atls::MeasuresExtern(t::id_of("A")), {"A"}, t::daily_events({t::walk("A", 9, 300)}));
  const auto in = atl::backtest::measure_input(outcome.result_, atl::backtest::NextBarFillModel{}, 252.0);
  const ByHand h = by_hand(outcome.result_);
  REQUIRE(h.defined_ > 100);
  atls::user::counted_calls = 0;
  const double c = atls::MeasuresExtern::measure_c(in);
  CHECK(atls::user::counted_calls == h.defined_);
  CHECK(!std::isnan(c));
}

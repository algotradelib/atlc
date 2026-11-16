// The reductions of atls/measures.hpp over hand-built equity curves: missing returns, an empty curve, a single point
// and non-positive equity, and agreement with the library's own maximum drawdown and Sharpe ratio.
#include <atls/measures.hpp>

#include <algotradelib/analytics>
#include <algotradelib/backtest>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_floating_point.hpp>

#include <chrono>
#include <cmath>
#include <cstdint>
#include <vector>

namespace {

namespace atl = algotradelib;
namespace m = atls::measures;
using Catch::Matchers::WithinRel;

const double NaN = atls::ops::NaN;

/** A replay whose curve has these levels and period returns (NaN: missing), one point a day. */
atl::backtest::ReplayResult curve(const std::vector<double>& levels, const std::vector<double>& returns) {
  atl::backtest::ReplayResult result;
  for (std::size_t i = 0; i < levels.size(); ++i) {
    atl::backtest::EquityPoint point;
    point.at_ = std::chrono::system_clock::time_point{} + std::chrono::hours(24 * static_cast<int>(i));
    point.equity_ = atl::Decimal64(static_cast<std::int64_t>(std::llround(levels[i] * 100)), 2);
    point.period_return_ = std::isnan(returns[i]) ? atl::math::missing() : returns[i];
    result.equity_.push_back(point);
  }
  return result;
}

double ret(const m::Point& p) {
  return p.ret_;
}

double equity(const m::Point& p) {
  return p.equity_;
}

double drawdown(const m::Point& p) {
  return p.drawdown_;
}

} // namespace

TEST_CASE("each reduction skips the missing returns") {
  const auto result = curve({100, 110, 99, 99, 120}, {NaN, 0.1, -0.1, NaN, 0.25});
  const auto in = atl::backtest::measure_input(result, atl::backtest::NextBarFillModel{}, 252.0);
  CHECK(m::sum(in, ret) == 0.1 + -0.1 + 0.25);
  CHECK(m::mean(in, ret) == (0.1 + -0.1 + 0.25) / 3);
  const double mean = (0.1 + -0.1 + 0.25) / 3;
  const double squares = (0.1 - mean) * (0.1 - mean) + (-0.1 - mean) * (-0.1 - mean) + (0.25 - mean) * (0.25 - mean);
  CHECK_THAT(m::stdev(in, ret), WithinRel(std::sqrt(squares / 2), 1e-15));
  CHECK(m::min(in, ret) == -0.1);
  CHECK(m::max(in, ret) == 0.25);
  CHECK(m::first(in, ret) == 0.1);
  CHECK(m::last(in, ret) == 0.25);
  CHECK(m::count(in, [](const m::Point& p) {
    return std::isnan(p.ret_) ? NaN : (p.ret_ > 0 ? 1.0 : 0.0);
  }) == 2);
  CHECK(m::count(in, [](const m::Point&) {
    return 1.0;
  }) == 5);
  CHECK(m::first(in, equity) == 100);
  CHECK(m::last(in, equity) == 120);
  CHECK(m::max(in, drawdown) == (110.0 - 99) / 110);
  CHECK(m::periods_per_year(in) == 252);
  CHECK(m::fills(in) == 0);
}

TEST_CASE("an empty curve sums and counts to 0, and the rest is undefined") {
  const atl::backtest::ReplayResult result;
  const auto in = atl::backtest::measure_input(result, atl::backtest::NextBarFillModel{});
  CHECK(m::sum(in, ret) == 0);
  CHECK(m::count(in, ret) == 0);
  for (
    const double v :
    {m::mean(in, ret), m::stdev(in, ret), m::min(in, ret), m::max(in, ret), m::first(in, ret), m::last(in, ret),
      m::max(in, drawdown), m::periods_per_year(in)}
  ) {
    CHECK(std::isnan(v));
  }
}

TEST_CASE("a single point has no deviation") {
  const auto result = curve({100}, {0.5});
  const auto in = atl::backtest::measure_input(result, atl::backtest::NextBarFillModel{}, 252.0);
  CHECK(m::mean(in, ret) == 0.5);
  CHECK(std::isnan(m::stdev(in, ret)));
  CHECK(m::first(in, ret) == m::last(in, ret));
  CHECK(m::max(in, drawdown) == 0);
}

TEST_CASE("a drawdown is undefined under a peak at or below 0, as the library's") {
  const auto result = curve({-5, -10, 10, 5}, {NaN, NaN, NaN, -0.5});
  const auto in = atl::backtest::measure_input(result, atl::backtest::NextBarFillModel{});
  std::vector<double> seen;
  m::each(in, [&](const m::Point& p) {
    seen.push_back(p.drawdown_);
  });
  REQUIRE(seen.size() == 4);
  CHECK(std::isnan(seen[0]));
  CHECK(std::isnan(seen[1]));
  CHECK(seen[2] == 0);
  CHECK(seen[3] == 0.5);
  CHECK(m::max(in, drawdown) == atl::analytics::max_drawdown(in.levels_).magnitude_);
}

TEST_CASE("the maximum drawdown and the Sharpe ratio equal the library's") {
  std::vector<double> levels{1000};
  std::vector<double> returns{NaN};
  for (int i = 1; i < 300; ++i) {
    const double r = std::sin(i * 0.7) * 0.02 + 0.0005;
    levels.push_back(std::round(levels.back() * (1 + r) * 100) / 100);
    returns.push_back(levels[i] / levels[i - 1] - 1);
  }
  const auto result = curve(levels, returns);
  const auto in = atl::backtest::measure_input(result, atl::backtest::NextBarFillModel{}, 252.0);
  CHECK_THAT(m::max(in, drawdown), WithinRel(atl::analytics::max_drawdown(in.levels_).magnitude_, 1e-12));
  CHECK_THAT(m::mean(in, ret) / m::stdev(in, ret), WithinRel(atl::analytics::sharpe(in.returns_), 1e-12));
}

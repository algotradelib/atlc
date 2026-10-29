// Single-instrument programs (tests/programs/), each generated in every arithmetic, without shared rings and with
// the bar history kept: they compile as library strategies and trade the same fills on generated bars.
#include "replay.hpp"

#include <mean_reversion_exact.hpp>
#include <mean_reversion_incremental.hpp>
#include <mean_reversion_keep.hpp>
#include <mean_reversion_naive.hpp>
#include <mean_reversion_noshare.hpp>
#include <trailing_stop_exact.hpp>
#include <trailing_stop_incremental.hpp>
#include <trailing_stop_keep.hpp>
#include <trailing_stop_naive.hpp>
#include <trailing_stop_noshare.hpp>
#include <user_indicator_exact.hpp>
#include <user_indicator_incremental.hpp>
#include <user_indicator_keep.hpp>
#include <user_indicator_naive.hpp>
#include <user_indicator_noshare.hpp>

#include <catch2/catch_test_macros.hpp>

namespace {

namespace atl = algotradelib;
namespace t = atlc_test;

ATL_ASSERT_STRATEGY(atls::MeanReversionIncremental, atl::strategy::MinimalContext);
ATL_ASSERT_STRATEGY(atls::TrailingStopIncremental, atl::strategy::MinimalContext);
ATL_ASSERT_STRATEGY(atls::UserIndicatorIncremental, atl::strategy::MinimalContext);
static_assert(!atls::MeanReversionIncremental::reads_bar_history);

/** Every variant's fills and report equal the first's; returns the first. */
t::Outcome same_fills(const std::vector<t::Outcome>& runs) {
  for (std::size_t k = 1; k < runs.size(); ++k) {
    INFO("variant " << k);
    CHECK(t::signature(runs[k]) == t::signature(runs[0]));
  }
  const auto& first = runs.front();
  CHECK(first.report_.rejected_ == 0);
  CHECK(first.report_.refused_ == 0);
  return first;
}

std::size_t count_side(const t::Outcome& outcome, atl::OrderSide side) {
  std::size_t n = 0;
  for (const auto& e : outcome.result_.executions_) {
    n += e.fill_.side_ == side;
  }
  return n;
}

} // namespace

TEST_CASE("mean reversion: one instrument, the same fills in every variant") {
  const auto bars = t::walk("A", 1, 1500);
  const auto events = t::daily_events({bars});
  const auto id = t::id_of("A");
  const auto first = same_fills({
    t::run(atls::MeanReversionIncremental(id), {"A"}, events),
    t::run(atls::MeanReversionNaive(id), {"A"}, events),
    t::run(atls::MeanReversionExact(id), {"A"}, events),
    t::run(atls::MeanReversionNoshare(id), {"A"}, events),
    t::run(atls::MeanReversionKeep(id), {"A"}, events),
  });
  CHECK(count_side(first, atl::OrderSide::BUY) > 5);
  CHECK(count_side(first, atl::OrderSide::SELL) > 5);
}

TEST_CASE("parameters are set by name and checked against their bounds") {
  const auto id = t::id_of("A");
  atls::MeanReversionIncremental s(id);
  CHECK(s.warmup_bars() == 20);
  s.set("lookback", 30);
  CHECK(s.warmup_bars() == 30);
  CHECK_THROWS_AS(s.set("lookback", 1), std::invalid_argument);
  CHECK_THROWS_AS(s.set("lookback", 2.5), std::invalid_argument);
  CHECK_THROWS_AS(s.set("entry", -1), std::invalid_argument);
  CHECK_THROWS_AS(s.set("nope", 1), std::invalid_argument);
  atls::MeanReversionIncremental::Params bad;
  bad.lookback_ = 1;
  CHECK_THROWS_AS(atls::MeanReversionIncremental(id, bad), std::invalid_argument);
  CHECK(atls::MeanReversionIncremental::atls_params() == std::vector<std::string>{"lookback", "entry", "exit", "quantity"});
}

TEST_CASE("trailing stop: state and on_fill, the same fills in every variant") {
  const auto bars = t::walk("A", 2, 1500);
  const auto events = t::daily_events({bars});
  const auto id = t::id_of("A");
  const auto first = same_fills({
    t::run(atls::TrailingStopIncremental(id), {"A"}, events),
    t::run(atls::TrailingStopNaive(id), {"A"}, events),
    t::run(atls::TrailingStopExact(id), {"A"}, events),
    t::run(atls::TrailingStopNoshare(id), {"A"}, events),
    t::run(atls::TrailingStopKeep(id), {"A"}, events),
  });
  // Every buy is followed by a sell before the next buy: on_fill opened and closed each trade.
  CHECK(count_side(first, atl::OrderSide::BUY) > 5);
  bool open = false;
  for (const auto& e : first.result_.executions_) {
    CHECK(open == (e.fill_.side_ == atl::OrderSide::SELL));
    open = e.fill_.side_ == atl::OrderSide::BUY;
  }
}

TEST_CASE("a user indicator with two values in one instance, and a function: the same fills in every variant") {
  const auto bars = t::walk("A", 3, 1500);
  const auto events = t::daily_events({bars});
  const auto id = t::id_of("A");
  const auto first = same_fills({
    t::run(atls::UserIndicatorIncremental(id), {"A"}, events),
    t::run(atls::UserIndicatorNaive(id), {"A"}, events),
    t::run(atls::UserIndicatorExact(id), {"A"}, events),
    t::run(atls::UserIndicatorNoshare(id), {"A"}, events),
    t::run(atls::UserIndicatorKeep(id), {"A"}, events),
  });
  CHECK(count_side(first, atl::OrderSide::BUY) > 5);
  // The function caps every position at `cap` shares.
  for (const auto& e : first.result_.executions_) {
    CHECK(atl::decimal_less(e.fill_.quantity_, atl::Decimal64(501, 0)));
  }
}

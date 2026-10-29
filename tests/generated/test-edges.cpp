// Small programs at the edges of the generated code (tests/generated/programs/), each the regression test of one
// defect the generated code had.
#include "replay.hpp"

#include <contraction.hpp>
#include <integers.hpp>
#include <missing_open.hpp>
#include <universe_function.hpp>

#include <catch2/catch_test_macros.hpp>

#include <stdexcept>

namespace {

namespace atl = algotradelib;
namespace t = atlc_test;

/** One daily bar of `symbol` closing at `close`, with or without an open. */
atl::strategy::BarEvent bar_of(const std::string& symbol, std::int64_t close, bool open) {
  atl::Bar bar;
  bar.timestamp_ = std::chrono::system_clock::time_point{} + std::chrono::hours(24 * 18262);
  bar.high_ = bar.low_ = bar.close_ = atl::Decimal64(close, 0);
  if (open) {
    bar.open_ = bar.close_;
  }
  bar.volume_ = atl::Decimal64(1000, 0);
  return atl::strategy::make_bar_event(t::id_of(symbol), atl::BarInterval::ONE_DAY, atl::BarStamp::OPEN, bar);
}

/** The shares `on_bar` targets on one bar, or nothing. */
template <class S>
std::optional<std::string> target_of(S& strategy, bool open = true) {
  atl::strategy::MinimalContext ctx;
  strategy.on_bar(ctx, bar_of("A", 10000, open));
  if (ctx.targets().empty()) {
    return std::nullopt;
  }
  return ctx.targets().back().target_quantity_.str();
}

} // namespace

TEST_CASE("a comparison with a missing open is undefined: `!= ... ? ... : keep` states nothing") {
  atls::MissingOpen strategy(t::id_of("A"));
  CHECK(target_of(strategy, false) == std::nullopt);
  CHECK(target_of(strategy, true) == std::optional<std::string>("10"));
}

TEST_CASE("integer arithmetic is 64-bit, and a result past 64 bits throws instead of trading") {
  const auto id = t::id_of("A");
  const auto with = [&id](double which, double c) {
    return t::with(atls::Integers(id), {{"which", which}, {"c", c}});
  };
  // Two literals whose product needs more than 32 bits.
  auto product = with(0, 0);
  CHECK(target_of(product) == std::optional<std::string>("2500000000"));
  // 3037000499 squared is 9,223,372,030,926,249,001; 5,928,526,806 more is the largest 64-bit integer.
  auto plus = with(1, 5928526806);
  CHECK(target_of(plus) == std::optional<std::string>("5928526806"));
  auto plus_over = with(1, 5928526807);
  CHECK_THROWS_AS(target_of(plus_over), std::overflow_error);
  // The exception leaves the backtester: the run stops.
  CHECK_THROWS_AS(t::run(with(1, 5928526807), {"A"}, t::daily_events({t::walk("A", 1, 5)})), std::overflow_error);
  // -square - 5,928,526,807 is the smallest 64-bit integer; one less does not fit, nor does its negation.
  auto minus = with(2, 5928526807);
  CHECK(target_of(minus) == std::optional<std::string>("-5928526807"));
  auto minus_over = with(2, 5928526808);
  CHECK_THROWS_AS(target_of(minus_over), std::overflow_error);
  auto negate = with(3, 5928526806);
  CHECK(target_of(negate) == std::optional<std::string>("5928526806"));
  auto negate_over = with(3, 5928526807);
  CHECK_THROWS_AS(target_of(negate_over), std::overflow_error);
  // The largest integer literal and default (9e15), and a product in a state's initial value.
  auto literals = with(4, 0);
  CHECK(target_of(literals) == std::optional<std::string>("7"));
  // 3037000500 squared does not fit: the state's initial value throws when the strategy is set up.
  atls::Integers::Params over;
  over.a_ = 3037000500;
  over.b_ = 3037000500;
  CHECK_THROWS_AS(atls::Integers(id, over), std::overflow_error);
}

TEST_CASE("a cross-sectional value passed to a function") {
  const std::vector<t::Series> all{t::walk("A", 21, 30), t::walk("B", 22, 30)};
  const std::vector<atl::InstrumentId> ids{t::id_of("A"), t::id_of("B")};
  const auto outcome = t::run(atls::UniverseFunction(ids), {"A", "B"}, t::daily_events(all));
  REQUIRE(outcome.result_.executions_.size() >= 2);
  // Both instruments buy a hundredth of the first day's mean close, truncated.
  const double mean = (std::stod(all[0].bars_[0].close_.str()) + std::stod(all[1].bars_[0].close_.str())) / 2;
  const auto shares = std::to_string(static_cast<std::int64_t>(mean / 100));
  CHECK(outcome.result_.executions_[0].fill_.quantity_.str() == shares);
  CHECK(outcome.result_.executions_[1].fill_.quantity_.str() == shares);
}

TEST_CASE("state initialization, on_start and on_stop round a * b + c twice, as the handlers do") {
  atls::Contraction strategy(t::id_of("A"));
  atl::strategy::MinimalContext ctx;
  CHECK(strategy.st_at_setup_ == 0.0);
  strategy.on_start(ctx);
  CHECK(strategy.st_at_start_ == 0.0);
  strategy.on_stop(ctx);
  CHECK(strategy.st_at_stop_ == 0.0);
}

// Programs with `data {}` (tests/generated/programs/data-*.atls): values reach series and handlers as known by
// each bar's end, whatever order records and bars are handed over in.
#include "replay.hpp"

#include <data_named.hpp>
#include <data_series.hpp>
#include <data_single.hpp>
#include <data_universe.hpp>

#include <catch2/catch_test_macros.hpp>

#include <chrono>
#include <optional>
#include <stdexcept>
#include <string>

namespace {

namespace atl = algotradelib;
namespace t = atlc_test;
using Clock = std::chrono::system_clock;

ATL_ASSERT_STRATEGY(atls::DataSingle, atl::strategy::MinimalContext);
ATL_ASSERT_STRATEGY(atls::DataUniverse, atl::strategy::MinimalContext);

/** Day `d` of the test's calendar, at its close. */
Clock::time_point day(int d) {
  return Clock::time_point{} + std::chrono::hours(24 * (18262 + d));
}

/** A close-stamped daily bar of `symbol` ending at `end`. */
atl::strategy::BarEvent bar_at(const std::string& symbol, Clock::time_point end) {
  atl::Bar bar;
  bar.timestamp_ = end;
  bar.open_ = atl::Decimal64(100, 0);
  bar.high_ = bar.low_ = bar.close_ = atl::Decimal64(100, 0);
  bar.volume_ = atl::Decimal64(1000, 0);
  return atl::strategy::make_bar_event(t::id_of(symbol), atl::BarInterval::ONE_DAY, atl::BarStamp::CLOSE, bar);
}

/** A record of value `name` known at `at`, for `symbol` or for every instrument. */
atl::strategy::DataEvent<atls::DataRecord> record(
  Clock::time_point at, double value, std::size_t name = 0, std::optional<std::string> symbol = std::nullopt
) {
  atls::DataRecord r;
  r.name_ = name;
  r.value_ = value;
  if (symbol) {
    r.instrument_ = t::id_of(*symbol);
  }
  return atl::strategy::DataEvent<atls::DataRecord>{at, r};
}

/** The shares the last target states, or nothing. */
std::optional<std::string> last_target(const atl::strategy::MinimalContext& ctx, std::size_t before) {
  if (ctx.targets().size() == before) {
    return std::nullopt;
  }
  return ctx.targets().back().target_quantity_.str();
}

/** Feeds one bar to `on_bar` and returns what it targeted. */
template <class S>
std::optional<std::string> bar(S& strategy, atl::strategy::MinimalContext& ctx, Clock::time_point end) {
  const auto before = ctx.targets().size();
  strategy.on_bar(ctx, bar_at("A", end));
  return last_target(ctx, before);
}

} // namespace

TEST_CASE("data: the program states what it reads") {
  STATIC_REQUIRE(atls::DataSingle::atls_reads_data);
  CHECK(atls::DataSingle::atls_data() == std::vector<std::string>{"level"});
}

TEST_CASE("data: undefined before the first record, then the latest known by each bar's end") {
  atls::DataSingle strategy(t::id_of("A"));
  atl::strategy::MinimalContext ctx;
  CHECK(bar(strategy, ctx, day(0)) == std::nullopt);
  strategy.on_data(ctx, record(day(0) + std::chrono::hours(1), 5));
  CHECK(bar(strategy, ctx, day(1)) == std::optional<std::string>("5"));
  // Received before the next bar is handed over, but known only after it ended (a backlog): not read by it.
  strategy.on_data(ctx, record(day(2) + std::chrono::hours(1), 7));
  CHECK(bar(strategy, ctx, day(2)) == std::optional<std::string>("5"));
  CHECK(bar(strategy, ctx, day(3)) == std::optional<std::string>("7"));
  // A record at a bar's own end is known by it; an undefined value makes the value undefined again.
  strategy.on_data(ctx, record(day(4), 9));
  CHECK(bar(strategy, ctx, day(4)) == std::optional<std::string>("9"));
  strategy.on_data(ctx, record(day(4) + std::chrono::hours(1), atls::ops::NaN));
  CHECK(bar(strategy, ctx, day(5)) == std::nullopt);
}

TEST_CASE("data: several records between two bars, the bar reads the last") {
  atls::DataSingle strategy(t::id_of("A"));
  atl::strategy::MinimalContext ctx;
  strategy.on_data(ctx, record(day(0) + std::chrono::minutes(5), 1));
  strategy.on_data(ctx, record(day(0) + std::chrono::minutes(7), 2));
  strategy.on_data(ctx, record(day(0) + std::chrono::minutes(613), 3));
  CHECK(bar(strategy, ctx, day(1)) == std::optional<std::string>("3"));
}

TEST_CASE("data: a series over a value counts bars, and a later record never changes an earlier bar") {
  atls::DataSeries strategy(t::id_of("A"));
  atl::strategy::MinimalContext ctx;
  strategy.on_data(ctx, record(day(0), 10));
  CHECK(bar(strategy, ctx, day(0)) == std::nullopt);
  // One record, two bars: the mean of 10 and 10.
  CHECK(bar(strategy, ctx, day(1)) == std::optional<std::string>("10"));
  strategy.on_data(ctx, record(day(3) + std::chrono::hours(1), 30));
  CHECK(bar(strategy, ctx, day(2)) == std::optional<std::string>("10"));
  CHECK(bar(strategy, ctx, day(4)) == std::optional<std::string>("20"));
}

TEST_CASE("data: a record naming another instrument, or no value of data {}, is refused") {
  atls::DataSingle strategy(t::id_of("A"));
  atl::strategy::MinimalContext ctx;
  CHECK_NOTHROW(strategy.on_data(ctx, record(day(0), 1, 0, "A")));
  CHECK_THROWS_AS(strategy.on_data(ctx, record(day(0), 1, 0, "B")), std::invalid_argument);
  CHECK_THROWS_AS(strategy.on_data(ctx, record(day(0), 1, 1)), std::invalid_argument);
}

TEST_CASE("data: set() empties what was received") {
  atls::DataSingle strategy(t::id_of("A"));
  atl::strategy::MinimalContext ctx;
  strategy.on_data(ctx, record(day(0), 5));
  strategy.setup();
  CHECK(bar(strategy, ctx, day(1)) == std::nullopt);
}

TEST_CASE("data: a named program takes market-wide values only") {
  atls::DataNamed strategy({t::id_of("Y"), t::id_of("X")});
  atl::strategy::MinimalContext ctx;
  CHECK_THROWS_AS(strategy.on_data(ctx, record(day(0), 1, 0, "Y")), std::invalid_argument);
  strategy.on_data(ctx, record(day(0), 4));
  atl::strategy::BarSlice slice;
  slice.bars_ = {bar_at("Y", day(1)), bar_at("X", day(1))};
  slice.period_end_ = day(1);
  strategy.on_bars(ctx, slice);
  REQUIRE(ctx.targets().size() == 2);
  CHECK(ctx.targets()[0].target_quantity_.str() == "4");
}

TEST_CASE("data: a universe's value is per instrument; a record naming none sets every one") {
  atls::DataUniverse strategy({t::id_of("A"), t::id_of("B")});
  atl::strategy::MinimalContext ctx;
  strategy.on_data(ctx, record(day(0), 1));
  strategy.on_data(ctx, record(day(0) + std::chrono::hours(1), 2, 0, "B"));
  CHECK_THROWS_AS(strategy.on_data(ctx, record(day(0), 3, 0, "C")), std::invalid_argument);
  atl::strategy::BarSlice slice;
  slice.bars_ = {bar_at("A", day(1)), bar_at("B", day(1))};
  slice.period_end_ = day(1);
  strategy.on_bars(ctx, slice);
  REQUIRE(ctx.targets().size() == 2);
  for (const auto& target : ctx.targets()) {
    CHECK(target.target_quantity_.str() == (target.instrument_.symbol_ == "A" ? "1" : "2"));
  }
}

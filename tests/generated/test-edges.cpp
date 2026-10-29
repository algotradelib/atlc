// Small programs at the edges of the generated code (tests/generated/programs/), each the regression test of one
// defect the generated code had.
#include "replay.hpp"

#include <missing_open.hpp>

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

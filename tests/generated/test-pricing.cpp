// Limit pricing in generated strategies (tests/generated/programs/pricing*.atls): the target each shape submits and
// the price it carries, the refusals, the library's reconciliation of that price against a band, and the fills the
// backtester gives it.
#include "replay.hpp"

#include <mean_reversion_incremental.hpp>
#include <pricing.hpp>
#include <pricing_held.hpp>
#include <pricing_legs.hpp>
#include <pricing_legs_held.hpp>
#include <pricing_named.hpp>
#include <pricing_named_held.hpp>
#include <pricing_once.hpp>
#include <pricing_universe.hpp>
#include <pricing_universe_held.hpp>
#include <pricing_universe_poison.hpp>
#include <pricing_universe_skip.hpp>
#include <universe_incremental.hpp>

#include <catch2/catch_test_macros.hpp>

#include <chrono>
#include <cstdint>
#include <limits>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace {

namespace atl = algotradelib;
namespace t = atlc_test;
using Ctx = atl::strategy::MinimalContext;

/** 2020-01-01 00:00 UTC. */
const auto FIRST_DAY = std::chrono::system_clock::time_point{} + std::chrono::hours(24 * 18262);

/** A Korean stock quoted on the KRX tick table, or on `rule`. */
atl::Instrument instrument(const std::string& symbol, atl::PriceRule rule = atl::PriceRule::krx_equity()) {
  atl::Instrument out;
  out.id_ = t::id_of(symbol);
  out.name_ = symbol;
  out.type_ = atl::SecurityType::STOCK;
  out.currency_ = atl::Currency{"KRW"};
  out.lot_size_ = atl::Decimal64(1, 0);
  out.price_rule_ = std::move(rule);
  return out;
}

/** A grid of one tick everywhere. */
atl::PriceRule ticks_of(std::int64_t tick) {
  atl::PriceRule rule;
  rule.kind_ = atl::PriceRule::Kind::STATED;
  rule.bands_ = {atl::TickBand{atl::Decimal64(0, 0), atl::Decimal64(tick, 0)}};
  rule.source_ = "test";
  return rule;
}

/** A decimal from its text. */
atl::Decimal64 dec(const char* text) { return atl::Decimal64::parse(text); }

/** One daily bar, open = high = low = close, `day` days after the first. */
atl::strategy::BarEvent daily(const std::string& symbol, const atl::Decimal64& close, int day = 0) {
  atl::Bar bar;
  bar.timestamp_ = FIRST_DAY + std::chrono::hours(24 * day);
  bar.open_ = close;
  bar.high_ = bar.low_ = bar.close_ = close;
  bar.volume_ = atl::Decimal64(100000, 0);
  return atl::strategy::make_bar_event(t::id_of(symbol), atl::BarInterval::ONE_DAY, atl::BarStamp::OPEN, bar);
}

/** One slice of the bars given. */
atl::strategy::BarSlice slice_of(std::vector<atl::strategy::BarEvent> bars) {
  atl::strategy::BarSlice out;
  out.period_end_ = bars.front().period_end_;
  out.interval_ = bars.front().interval_;
  out.bars_ = std::move(bars);
  return out;
}

/** Events grouped into one slice per period, as a runner hands them to `on_bars`. */
std::vector<atl::strategy::BarSlice> slices_of(const std::vector<atl::strategy::BarEvent>& events) {
  std::vector<atl::strategy::BarSlice> out;
  for (const auto& e : events) {
    if (out.empty() || out.back().period_end_ != e.period_end_) {
      out.push_back(slice_of({e}));
    } else {
      out.back().bars_.push_back(e);
    }
  }
  return out;
}

/** A holding row of `quantity` shares. */
void hold(Ctx& ctx, const std::string& symbol, const atl::Decimal64& quantity) {
  atl::HoldingPosition row;
  row.instrument_ = t::id_of(symbol);
  row.quantity_ = quantity;
  ctx.portfolio_.positions_.push_back(row);
}

/** A probe knowing the instruments named. */
Ctx probe(const std::vector<std::string>& symbols = {"A"}) {
  Ctx ctx;
  for (const auto& s : symbols) {
    ctx.instruments_.push_back(instrument(s));
  }
  return ctx;
}

/** A target as `SYMBOL QUANTITY @ PRICE`, `@ market` without a limit. */
std::string text_of(const atl::strategy::TargetPosition& target) {
  return target.instrument_.symbol_ + " " + target.target_quantity_.str() + " @ " + (target.limit_ ? target.limit_->str() : "market");
}

/** Every target a probe got, as text. */
std::vector<std::string> targets_of(const Ctx& ctx) {
  std::vector<std::string> out;
  for (const auto& target : ctx.targets()) {
    out.push_back(text_of(target));
  }
  return out;
}

/** A strategy with limit pricing at `offset`. */
template <class S>
S limit(S strategy, const char* offset) {
  strategy.set_order_pricing(atl::OrderType::LIMIT, dec(offset));
  return strategy;
}

/** What the single-instrument program submits on one bar at `close`, holding `rows`. */
template <class S>
std::vector<std::string> one_bar(S& strategy, const atl::Decimal64& close, const std::vector<atl::Decimal64>& rows = {}, Ctx ctx = probe()) {
  for (const auto& row : rows) {
    hold(ctx, "A", row);
  }
  strategy.on_bar(ctx, daily("A", close));
  return targets_of(ctx);
}

using Texts = std::vector<std::string>;

} // namespace

TEST_CASE("one instrument: the side from the truncated target against the exact net, the price from the bar") {
  for (const bool reads_held : {false, true}) {
    INFO("reads held: " << reads_held);
    const auto check = [&](double want, const std::vector<atl::Decimal64>& rows, const Texts& expected) {
      INFO("want " << want);
      if (reads_held) {
        auto s = limit(t::with(atls::PricingHeld(t::id_of("A")), {{"want", want}}), "0.0015");
        CHECK(one_bar(s, dec("10000"), rows) == expected);
      } else {
        auto s = limit(t::with(atls::Pricing(t::id_of("A")), {{"want", want}}), "0.0015");
        CHECK(one_bar(s, dec("10000"), rows) == expected);
      }
    };
    // From flat: a buy at 10000 × 1.0015 = 10015, down to the tick of 10: 10010; a sell at 9985, up: 9990.
    check(10, {}, {"A 10 @ 10010"});
    check(-10, {}, {"A -10 @ 9990"});
    // Truncated toward zero before the side is decided.
    check(10.7, {}, {"A 10 @ 10010"});
    check(-10.7, {}, {"A -10 @ 9990"});
    check(0.9, {}, {"A 0 @ market"});
    // A short growing sells, shrinking buys; across zero by the same comparison.
    check(-20, {dec("-10")}, {"A -20 @ 9990"});
    check(-5, {dec("-10")}, {"A -5 @ 10010"});
    check(-10, {dec("10")}, {"A -10 @ 9990"});
    check(10, {dec("-10")}, {"A 10 @ 10010"});
    // Equal to the position (summed over its rows): submitted without a limit.
    check(10, {dec("10")}, {"A 10 @ market"});
    check(10, {dec("6"), dec("4")}, {"A 10 @ market"});
    check(12, {dec("6"), dec("4")}, {"A 12 @ 10010"});
    check(8, {dec("6"), dec("4")}, {"A 8 @ 9990"});
  }
  // Offset 0: the bar's price itself, on the grid.
  auto zero = limit(atls::Pricing(t::id_of("A")), "0");
  CHECK(one_bar(zero, dec("10000")) == Texts{"A 10 @ 10000"});
}

TEST_CASE("a net position that does not sum is refused, never read as flat") {
  const std::vector<atl::Decimal64> rows{atl::Decimal64(std::numeric_limits<std::int64_t>::max(), 0), dec("1")};
  auto s = limit(atls::Pricing(t::id_of("A")), "0.0015");
  CHECK(one_bar(s, dec("10000"), rows).empty());
  CHECK(s.pricing_refusals().net_not_representable_ == 1);
  CHECK(s.pricing_refusals().total() == 1);
  // A program that reads `held` stops before its handler, as it always has: nothing to price.
  auto held = limit(atls::PricingHeld(t::id_of("A")), "0.0015");
  CHECK(one_bar(held, dec("10000"), rows).empty());
  CHECK(held.pricing_refusals().total() == 0);
}

TEST_CASE("an undefined target after a valid one states nothing; the valid one is priced once") {
  auto s = limit(t::with(atls::Pricing(t::id_of("A")), {{"undefined_after", 1}}), "0.0015");
  CHECK(one_bar(s, dec("10000")) == Texts{"A 10 @ 10010"});
  CHECK(s.pricing_refusals().total() == 0);
}

TEST_CASE("prices on tick boundaries are rounded on the safe side, by the band of the computed price") {
  const auto at = [](double want, const char* offset, const char* close) {
    auto s = limit(t::with(atls::Pricing(t::id_of("A")), {{"want", want}}), offset);
    const auto got = one_bar(s, dec(close));
    REQUIRE(got.size() == 1);
    return got.front().substr(got.front().find('@') + 2);
  };
  // The KRX table: a tick of 5 from 2000, of 10 from 5000, of 50 from 20000.
  CHECK(at(10, "0", "5000") == "5000");
  CHECK(at(10, "0", "5004.99") == "5000");
  CHECK(at(-10, "0", "4997.5") == "5000");
  CHECK(at(-10, "0", "5000.01") == "5010");
  CHECK(at(10, "0.001", "10000") == "10010");
  CHECK(at(-10, "0.001", "10000") == "9990");
  // 19990 × 1.0015 = 20019.985, in the band of 50: 20000.
  CHECK(at(10, "0.0015", "19990") == "20000");
  // The largest offset: 7 × 1.299999999999999999 = 9.099999999999999993, down to the tick of 1: 9.
  CHECK(at(10, "0.299999999999999999", "7") == "9");
  // Its coefficient has 19 digits, so most prices times it do not fit a decimal: refused, as below.
  auto wide = limit(atls::Pricing(t::id_of("A")), "0.299999999999999999");
  CHECK(one_bar(wide, dec("10000")).empty());
  CHECK(wide.pricing_refusals().price_not_representable_ == 1);
}

TEST_CASE("what cannot be priced is refused and counted, never sent as a market order") {
  struct Case {
    const char* what;
    double want;
    const char* offset;
    atl::Decimal64 close;
    std::vector<atl::Instrument> instruments;
    std::size_t atls::ops::PricingRefusals::*reason;
  };
  const auto max = std::numeric_limits<std::int64_t>::max();
  const std::vector<Case> cases{
    {"no reference data", 10, "0", dec("10000"), {}, &atls::ops::PricingRefusals::no_price_grid_},
    {"an unknown grid", 10, "0", dec("10000"), {instrument("A", atl::PriceRule::unknown("no table"))}, &atls::ops::PricingRefusals::no_price_grid_},
    {"a price of zero", 10, "0", dec("0"), {instrument("A")}, &atls::ops::PricingRefusals::no_basis_price_},
    {"a negative price", -10, "0", dec("-5"), {instrument("A")}, &atls::ops::PricingRefusals::no_basis_price_},
    {"a product that does not fit", 10, "0.0015", atl::Decimal64(max / 2, 0), {instrument("A")}, &atls::ops::PricingRefusals::price_not_representable_},
    {"rounding up that does not fit", -10, "0", atl::Decimal64(max, 0), {instrument("A")}, &atls::ops::PricingRefusals::price_not_representable_},
    {"a buy rounded down to zero", 10, "0", dec("5"), {instrument("A", ticks_of(10))}, &atls::ops::PricingRefusals::price_not_positive_},
  };
  for (const auto& c : cases) {
    INFO(c.what);
    auto s = limit(t::with(atls::Pricing(t::id_of("A")), {{"want", c.want}}), c.offset);
    Ctx ctx;
    ctx.instruments_ = c.instruments;
    CHECK(one_bar(s, c.close, {}, std::move(ctx)).empty());
    CHECK(s.pricing_refusals().*c.reason == 1);
    CHECK(s.pricing_refusals().total() == 1);
  }
  // Without pricing, the same bars submit market targets.
  auto plain = atls::Pricing(t::id_of("A"));
  CHECK(one_bar(plain, dec("10000"), {}, Ctx{}) == Texts{"A 10 @ market"});
  CHECK(plain.pricing_refusals().total() == 0);
}

TEST_CASE("the pricing is checked when set, and kept across set and setup") {
  atls::Pricing s(t::id_of("A"));
  CHECK_THROWS_AS(s.set_order_pricing(atl::OrderType::LIMIT, dec("0.3")), std::invalid_argument);
  CHECK_THROWS_AS(s.set_order_pricing(atl::OrderType::LIMIT, dec("-0.001")), std::invalid_argument);
  CHECK_THROWS_AS(s.set_order_pricing(atl::OrderType::MARKET, dec("0.001")), std::invalid_argument);
  CHECK_THROWS_AS(s.set_order_pricing(atl::OrderType::CONDITIONAL_LIMIT, dec("0")), std::invalid_argument);
  // 1 + 1e-30 needs 31 digits.
  CHECK_THROWS_AS(s.set_order_pricing(atl::OrderType::LIMIT, atl::Decimal64(1, 30)), std::invalid_argument);
  s.set_order_pricing(atl::OrderType::LIMIT, dec("0.299999999999999999"));
  s.set_order_pricing(atl::OrderType::LIMIT, dec("0.0015"));
  s.set("want", 20);
  s.setup();
  CHECK(one_bar(s, dec("10000")) == Texts{"A 20 @ 10010"});
  // Market again: no limit.
  s.set_order_pricing(atl::OrderType::MARKET, dec("0"));
  CHECK(one_bar(s, dec("10000")) == Texts{"A 20 @ market"});
}

TEST_CASE("named instruments: each target priced from its own instrument's bar") {
  const std::vector<atl::InstrumentId> ids{t::id_of("A"), t::id_of("B")};
  const auto slice = slice_of({daily("A", dec("10000")), daily("B", dec("50000"))});
  // B: a sell at 50000 × 0.9985 = 49925, up to the tick of 50: 49950.
  const Texts expected{"A 10 @ 10010", "B -5 @ 49950"};
  {
    auto s = limit(atls::PricingNamed(ids), "0.0015");
    auto ctx = probe({"A", "B"});
    s.on_bars(ctx, slice);
    CHECK(targets_of(ctx) == expected);
  }
  {
    auto s = limit(atls::PricingNamedHeld(ids), "0.0015");
    auto ctx = probe({"A", "B"});
    hold(ctx, "B", dec("-5"));
    s.on_bars(ctx, slice);
    // B's target equals its position: no limit.
    CHECK(targets_of(ctx) == Texts{"A 10 @ 10010", "B -5 @ market"});
  }
  // B without reference data: refused alone.
  auto s = limit(atls::PricingNamed(ids), "0.0015");
  auto ctx = probe({"A"});
  s.on_bars(ctx, slice);
  CHECK(targets_of(ctx) == Texts{"A 10 @ 10010"});
  CHECK(s.pricing_refusals().no_price_grid_ == 1);
}

TEST_CASE("a universe prices each lane from its own event, under every missing-value policy, without changing who is active") {
  const std::vector<atl::InstrumentId> ids{t::id_of("A"), t::id_of("B"), t::id_of("C")};
  // C has no bar: it is not active, so each active instrument targets 10 × 2.
  const auto slice = slice_of({daily("A", dec("10000")), daily("B", dec("50000"))});
  const auto run = [&](auto strategy, const std::vector<std::string>& known) {
    auto ctx = probe(known);
    strategy.on_bars(ctx, slice);
    return std::make_pair(targets_of(ctx), strategy.pricing_refusals().total());
  };
  const Texts priced{"A 20 @ 10010", "B 20 @ 50000"};
  const Texts plain{"A 20 @ market", "B 20 @ market"};
  CHECK(run(limit(atls::PricingUniverse(ids), "0.0015"), {"A", "B", "C"}) == std::make_pair(priced, std::size_t{0}));
  CHECK(run(limit(atls::PricingUniverseSkip(ids), "0.0015"), {"A", "B", "C"}) == std::make_pair(priced, std::size_t{0}));
  CHECK(run(limit(atls::PricingUniversePoison(ids), "0.0015"), {"A", "B", "C"}) == std::make_pair(priced, std::size_t{0}));
  CHECK(run(atls::PricingUniverse(ids), {"A", "B", "C"}) == std::make_pair(plain, std::size_t{0}));
  // Sells: A at 9985 up to 9990, B at 49925 up to 49950.
  const Texts sells{"A -20 @ 9990", "B -20 @ 49950"};
  CHECK(run(limit(t::with(atls::PricingUniverse(ids), {{"want", -10}}), "0.0015"), {"A", "B", "C"}) == std::make_pair(sells, std::size_t{0}));
  // B refused for want of a grid: A still counts B among the active instruments.
  CHECK(run(limit(atls::PricingUniverseSkip(ids), "0.0015"), {"A", "C"}) == std::make_pair(Texts{"A 20 @ 10010"}, std::size_t{1}));
  // Reading positions: A's two rows meet its target, B's net does not sum, so B is left out as it always was.
  auto held = limit(atls::PricingUniverseHeld(ids), "0.0015");
  auto ctx = probe({"A", "B", "C"});
  hold(ctx, "A", dec("6"));
  hold(ctx, "A", dec("4"));
  hold(ctx, "B", atl::Decimal64(std::numeric_limits<std::int64_t>::max(), 0));
  hold(ctx, "B", dec("1"));
  held.on_bars(ctx, slice);
  CHECK(targets_of(ctx) == Texts{"A 10 @ market"});
  CHECK(held.pricing_refusals().total() == 0);
}

TEST_CASE("session legs: the open's target from the open, the close's from the close, a late listing from its first open") {
  // A every day; C lists on day 3 with a gap: under `missing = skip` it takes the periods before as missing rows, so it
  // is active at its first open, with no event before it.
  t::Series a{"A", {}};
  t::Series c{"C", {}};
  for (int day = 0; day < 5; ++day) {
    atl::Bar bar;
    bar.timestamp_ = FIRST_DAY + std::chrono::hours(24 * day);
    bar.open_ = atl::Decimal64(10000 + 100 * day, 0);
    bar.close_ = atl::Decimal64(10050 + 100 * day, 0);
    bar.high_ = atl::Decimal64(10100 + 100 * day, 0);
    bar.low_ = atl::Decimal64(9950 + 100 * day, 0);
    bar.volume_ = atl::Decimal64(1000, 0);
    a.bars_.push_back(bar);
    if (day >= 3) {
      bar.open_ = atl::Decimal64(12000, 0);
      bar.high_ = atl::Decimal64(12100, 0);
      bar.low_ = atl::Decimal64(11900, 0);
      bar.close_ = atl::Decimal64(12050, 0);
      c.bars_.push_back(bar);
    }
  }
  const std::vector<atl::InstrumentId> ids{t::id_of("A"), t::id_of("C")};
  auto priced = limit(atls::PricingLegs(ids), "0.0015");
  auto held = limit(atls::PricingLegsHeld(ids), "0.0015");
  atls::PricingLegs plain(ids);
  std::vector<Texts> got;
  for (const auto& slice : slices_of(t::leg_events({a, c}))) {
    auto ctx_priced = probe({"A", "C"});
    auto ctx_held = probe({"A", "C"});
    auto ctx_plain = probe({"A", "C"});
    priced.on_bars(ctx_priced, slice);
    held.on_bars(ctx_held, slice);
    plain.on_bars(ctx_plain, slice);
    // Reading `held` in `on_close` reads the position the pricing compares with: the same targets and prices.
    CHECK(targets_of(ctx_held) == targets_of(ctx_priced));
    // The same targets, on the same legs, for the same instruments: pricing changes only the price.
    REQUIRE(ctx_priced.targets().size() == ctx_plain.targets().size());
    for (std::size_t k = 0; k < ctx_plain.targets().size(); ++k) {
      CHECK(ctx_priced.targets()[k].instrument_ == ctx_plain.targets()[k].instrument_);
      CHECK(ctx_priced.targets()[k].target_quantity_ == ctx_plain.targets()[k].target_quantity_);
      CHECK(!ctx_plain.targets()[k].limit_);
    }
    got.push_back(targets_of(ctx_priced));
  }
  // Three legs a day; the third (the close alone) runs no handler.
  REQUIRE(got.size() == 15);
  CHECK(got[0].empty());
  // Day 0's close: 10050 × 0.9985 = 10034.925, up to 10040.
  CHECK(got[1] == Texts{"A -10 @ 10040"});
  // Day 1's open: 10100 × 1.0015 = 10115.15, down to 10110.
  CHECK(got[3] == Texts{"A 10 @ 10110"});
  // Day 3's open: C at its first event, 12000 × 1.0015 = 12018 down to 12010; both active, so 10 × 2 each.
  CHECK(got[9] == Texts{"A 20 @ 10310", "C 20 @ 12010"});
  // Day 3's close: each from its own close.
  CHECK(got[10] == Texts{"A -10 @ 10340", "C -10 @ 12040"});
  for (std::size_t k = 2; k < got.size(); k += 3) {
    CHECK(got[k].empty());
  }
  CHECK(priced.pricing_refusals().total() == 0);
}

TEST_CASE("session legs over missing days: pricing changes no target's quantity, instrument or leg") {
  const std::vector<t::Series> all{t::walk("A", 21, 300, 9), t::walk("B", 22, 300, 9), t::walk("C", 23, 300, 9, 100)};
  std::vector<atl::InstrumentId> ids;
  for (const auto& s : all) {
    ids.push_back(t::id_of(s.symbol_));
  }
  auto priced = limit(atls::PricingLegs(ids), "0.01");
  atls::PricingLegs plain(ids);
  std::size_t targets = 0;
  for (const auto& slice : slices_of(t::leg_events(all))) {
    auto ctx_priced = probe({"A", "B", "C"});
    auto ctx_plain = probe({"A", "B", "C"});
    priced.on_bars(ctx_priced, slice);
    plain.on_bars(ctx_plain, slice);
    REQUIRE(ctx_priced.targets().size() == ctx_plain.targets().size());
    for (std::size_t k = 0; k < ctx_plain.targets().size(); ++k) {
      CHECK(ctx_priced.targets()[k].instrument_ == ctx_plain.targets()[k].instrument_);
      CHECK(ctx_priced.targets()[k].target_quantity_ == ctx_plain.targets()[k].target_quantity_);
      CHECK(ctx_priced.targets()[k].limit_.has_value());
    }
    targets += ctx_plain.targets().size();
  }
  CHECK(targets > 500);
  CHECK(priced.pricing_refusals().total() == 0);
}

TEST_CASE("the requested price through the library's reconciliation against a supplied band") {
  // A grid of 10 and a band the replay never supplies: clamped when aggressive, kept when passive, refused when a
  // clamp's rounding crosses the other bound.
  const auto inst = instrument("A", ticks_of(10));
  const auto plan = [&inst](double want, const char* close, const char* lower, const char* upper) {
    auto s = limit(t::with(atls::Pricing(t::id_of("A")), {{"want", want}}), "0");
    Ctx ctx;
    ctx.instruments_.push_back(inst);
    s.on_bar(ctx, daily("A", dec(close)));
    REQUIRE(ctx.targets().size() == 1);
    atl::strategy::ReconcileInputs inputs;
    inputs.instrument_ = &inst;
    atl::PriceLimits band;
    band.lower_ = dec(lower);
    band.upper_ = dec(upper);
    band.currency_ = atl::Currency{"KRW"};
    inputs.limits_ = band;
    return std::make_pair(ctx.targets().front(), atl::strategy::reconcile(ctx.targets().front(), ctx.portfolio_, inputs));
  };
  {
    // Aggressive: a buy at 120 above the band [101, 115], clamped to 115 and rounded down to 110.
    const auto [target, p] = plan(10, "120", "101", "115");
    REQUIRE(p.place_.size() == 1);
    CHECK(p.place_[0].price_ == dec("110"));
    CHECK(p.place_[0].price_clamped_);
  }
  {
    // Passive: a buy at 90 below the band, and a sell at 120 above it, kept.
    const auto [buy_target, buy] = plan(10, "90", "101", "109");
    REQUIRE(buy.place_.size() == 1);
    CHECK(buy.place_[0].price_ == dec("90"));
    const auto [sell_target, sell] = plan(-10, "120", "101", "109");
    REQUIRE(sell.place_.size() == 1);
    CHECK(sell.place_[0].price_ == dec("120"));
  }
  {
    // A buy at 120 clamped to 109 and rounded down to 100, across the lower bound: refused.
    const auto [target, p] = plan(10, "120", "101", "109");
    CHECK(p.place_.empty());
    REQUIRE(p.rejected_.size() == 1);
    CHECK(p.rejected_[0].reason_ == atl::strategy::RejectReason::NO_PERMITTED_PRICE);
  }
  {
    // The strategy's own rounding: 105 becomes 100 before the library sees it, so it arrives as a passive buy and is
    // kept, where the unrounded 105 would have been inside the band.
    const auto [target, p] = plan(10, "105", "101", "109");
    REQUIRE(target.limit_.has_value());
    CHECK(target.limit_->value() == dec("100"));
    REQUIRE(p.place_.size() == 1);
    CHECK(p.place_[0].price_ == dec("100"));
    CHECK(p.rejected_.empty());
  }
}

TEST_CASE("a target equal to the position goes without a price, so reconciliation still cancels working orders") {
  auto s = limit(atls::Pricing(t::id_of("A")), "0.0015");
  auto ctx = probe();
  hold(ctx, "A", dec("10"));
  atl::WorkingOrder<atl::strategy::ProbeBroker> working;
  working.order_.instrument_ = t::id_of("A");
  working.order_.side_ = atl::OrderSide::BUY;
  working.order_.status_ = atl::OrderStatus::PENDING;
  working.order_.quantity_ = dec("5");
  ctx.portfolio_.working_.push_back(working);
  // No grid is needed: the target is not priced.
  ctx.instruments_.clear();
  s.on_bar(ctx, daily("A", dec("10000")));
  REQUIRE(ctx.targets().size() == 1);
  CHECK(!ctx.targets().front().limit_);
  const auto inst = instrument("A");
  atl::strategy::ReconcileInputs inputs;
  inputs.instrument_ = &inst;
  const auto p = atl::strategy::reconcile(ctx.targets().front(), ctx.portfolio_, inputs);
  CHECK(p.cancel_.size() == 1);
  CHECK(p.place_.empty());
  CHECK(s.pricing_refusals().total() == 0);
}

namespace {

/** A day of `symbol`: open, high, low, close. */
atl::strategy::BarEvent day_of(int day, std::int64_t open, std::int64_t high, std::int64_t low, std::int64_t close) {
  atl::Bar bar;
  bar.timestamp_ = FIRST_DAY + std::chrono::hours(24 * day);
  bar.open_ = atl::Decimal64(open, 0);
  bar.high_ = atl::Decimal64(high, 0);
  bar.low_ = atl::Decimal64(low, 0);
  bar.close_ = atl::Decimal64(close, 0);
  bar.volume_ = atl::Decimal64(100000, 0);
  return atl::strategy::make_bar_event(t::id_of("A"), atl::BarInterval::ONE_DAY, atl::BarStamp::OPEN, bar);
}

/** One target on the first day's close, then what the backtester did with its order. */
t::Outcome once(double want, const char* offset, const std::vector<atl::strategy::BarEvent>& events, int slippage = 0, bool market = false) {
  auto s = t::with(atls::PricingOnce(t::id_of("A")), {{"want", want}});
  if (!market) {
    s.set_order_pricing(atl::OrderType::LIMIT, dec(offset));
  }
  t::Settings settings;
  settings.commission_ = "0";
  settings.sell_tax_ = "0";
  settings.slippage_ticks_ = slippage;
  settings.shorts_ = true;
  return t::run(std::move(s), {"A"}, events, settings);
}

} // namespace

TEST_CASE("fills: a limit fills only when the next bar reaches it, at the limit bounded into the bar") {
  const auto first = day_of(0, 10000, 10000, 10000, 10000);
  {
    // A buy at 10000; the next low is 11000: no fill, and the DAY order expires.
    const auto o = once(10, "0", {first, day_of(1, 11000, 11500, 11000, 11200)});
    CHECK(o.result_.executions_.empty());
    CHECK(o.report_.caveats_.expired_day_orders_ == 1);
    CHECK(o.report_.caveats_.unfilled_ == 0);
  }
  {
    // A sell at 10000; the next high is 9000: no fill.
    const auto o = once(-10, "0", {first, day_of(1, 9000, 9000, 8500, 8800)});
    CHECK(o.result_.executions_.empty());
    CHECK(o.report_.caveats_.expired_day_orders_ == 1);
  }
  {
    // The low equal to the limit fills, at the limit.
    const auto o = once(10, "0", {first, day_of(1, 10500, 10600, 10000, 10200)});
    REQUIRE(o.result_.executions_.size() == 1);
    CHECK(o.result_.executions_[0].fill_.price_ == dec("10000"));
  }
  const auto inside = std::vector<atl::strategy::BarEvent>{day_of(0, 10500, 10500, 10500, 10500), day_of(1, 10000, 11000, 9900, 10100)};
  {
    // Inside the range: credited the limit 10500, not the better open 10000 (`AT_LIMIT`).
    const auto o = once(10, "0", inside);
    REQUIRE(o.result_.executions_.size() == 1);
    CHECK(o.result_.executions_[0].fill_.price_ == dec("10500"));
    // A market order on the same bars fills at the open.
    const auto m = once(10, "0", inside, 0, true);
    REQUIRE(m.result_.executions_.size() == 1);
    CHECK(m.result_.executions_[0].fill_.price_ == dec("10000"));
  }
  {
    // Two ticks of slippage never take the price through the limit.
    const auto o = once(10, "0", inside, 2);
    REQUIRE(o.result_.executions_.size() == 1);
    CHECK(o.result_.executions_[0].fill_.price_ == dec("10500"));
  }
  {
    // An order placed on the last bar is still queued at the end: unfilled, not expired.
    const auto o = once(10, "0.0015", {first});
    CHECK(o.result_.executions_.empty());
    CHECK(o.report_.caveats_.unfilled_ == 1);
    CHECK(o.report_.caveats_.expired_day_orders_ == 0);
  }
}

TEST_CASE("market pricing set explicitly gives the default's fills exactly") {
  const auto one = t::daily_events({t::walk("A", 1, 1500)});
  auto market = atls::MeanReversionIncremental(t::id_of("A"));
  market.set_order_pricing(atl::OrderType::MARKET, dec("0"));
  CHECK(t::signature(t::run(market, {"A"}, one)) == t::signature(t::run(atls::MeanReversionIncremental(t::id_of("A")), {"A"}, one)));
  const std::vector<t::Series> all{t::walk("A", 6, 800, 15), t::walk("B", 7, 800, 15), t::walk("C", 8, 800, 15, 300), t::walk("D", 9, 800, 15)};
  const auto legs = t::leg_events(all);
  const std::vector<std::string> symbols{"A", "B", "C", "D"};
  std::vector<atl::InstrumentId> ids;
  for (const auto& s : symbols) {
    ids.push_back(t::id_of(s));
  }
  auto universe = atls::UniverseIncremental(ids);
  universe.set_order_pricing(atl::OrderType::MARKET, dec("0"));
  CHECK(t::signature(t::run(universe, symbols, legs)) == t::signature(t::run(atls::UniverseIncremental(ids), symbols, legs)));
}

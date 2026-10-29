// The runtime against its own recompute paths and the undefined-value rules: rings wrap, running sums match the
// window summed again on whole numbers, the monotonic queue matches a scan; and each indicator's arithmetic in
// each mode is the algorithm the reference names (docs/reference/types-and-definedness.md, "Arithmetic").
#include <atls/ops.hpp>
#include <atls/universe.hpp>

#include <catch2/catch_test_macros.hpp>

#include <cmath>
#include <random>
#include <vector>

// The expected values below are computed here, so they must round as the runtime does.
#if defined(__clang__)
#pragma clang fp contract(off)
#endif

namespace {

namespace ops = atls::ops;

std::vector<double> walk(std::size_t n, unsigned seed) {
  std::mt19937 rng(seed);
  std::vector<double> out;
  double x = 10000;
  for (std::size_t i = 0; i < n; ++i) {
    x += static_cast<double>(static_cast<int>(rng() % 101) - 50);
    out.push_back(x);
  }
  return out;
}

// Prices in cents, which a double cannot hold exactly: where running sums and sums again differ.
std::vector<double> cents(std::size_t n, unsigned seed) {
  std::mt19937 rng(seed);
  std::vector<double> out;
  long c = 2658;
  for (std::size_t i = 0; i < n; ++i) {
    c += static_cast<long>(rng() % 41) - 20;
    out.push_back(static_cast<double>(c) / 100);
  }
  return out;
}

bool same(double a, double b) { return (std::isnan(a) && std::isnan(b)) || a == b; }

} // namespace

TEST_CASE("a ring returns what was pushed, k pushes ago, across wraps") {
  ops::Ring ring;
  ring.reset(3);
  for (int i = 1; i <= 10; ++i) {
    ring.push(i);
    CHECK(ring.ago(0) == i);
    if (i >= 3) {
      CHECK(ring.ago(2) == i - 2);
    }
  }
  CHECK(ring.seen() == 10);
  ring.clear();
  CHECK(ring.seen() == 0);
}

TEST_CASE("incremental indicators equal the recomputed window on whole numbers") {
  for (std::size_t n : {1u, 2u, 3u, 7u, 20u}) {
    const auto xs = walk(500, static_cast<unsigned>(n));
    ops::Ring ring;
    ring.reset(n + 2);
    ops::Sma sma;
    ops::TsSum sum;
    ops::Rsi rsi;
    ops::Lag lag;
    ops::TsMax high;
    ops::TsMin low;
    sma.set(n);
    sum.set(n);
    rsi.set(n);
    lag.set(n);
    high.set(n);
    low.set(n);
    for (std::size_t i = 0; i < xs.size(); ++i) {
      if (i == 250) {
        // A restart, as an undefined input causes.
        ring.clear();
        sma.reset();
        sum.reset();
        rsi.reset();
        high.reset();
        low.reset();
      }
      ring.push(xs[i]);
      CHECK(same(sma.update(ring), sma.recompute(ring)));
      CHECK(same(sum.update(ring), sum.recompute(ring)));
      CHECK(same(rsi.update(ring), rsi.recompute(ring)));
      CHECK(same(lag.update(ring), ring.seen() > n ? xs[i - n] : ops::NaN));
      CHECK(same(high.update(xs[i]), high.recompute(ring)));
      CHECK(same(low.update(xs[i]), low.recompute(ring)));
    }
  }
}

TEST_CASE("subtraction-free sums equal the recomputed window on whole numbers, and leave no residue") {
  for (std::size_t n : {1u, 2u, 3u, 7u, 20u}) {
    const auto xs = walk(500, static_cast<unsigned>(n) + 100);
    ops::Ring ring;
    ring.reset(n + 2);
    ops::Sma sma;
    ops::SmaExact exact;
    ops::TsSum sum;
    ops::TsSumExact sum_exact;
    ops::Rsi rsi;
    ops::RsiExact rsi_exact;
    sma.set(n);
    exact.set(n);
    sum.set(n);
    sum_exact.set(n);
    rsi.set(n);
    rsi_exact.set(n);
    for (double x : xs) {
      ring.push(x);
      CHECK(same(exact.update(x), sma.recompute(ring)));
      CHECK(same(sum_exact.update(x), sum.recompute(ring)));
      CHECK(same(rsi_exact.update(x), rsi.recompute(ring)));
    }
  }
  // A running sum leaves a residue on non-integers; the window sum does not.
  ops::SmaExact one;
  one.set(1);
  ops::Ring ring;
  ring.reset(2);
  ops::Sma running;
  running.set(1);
  bool residue = false;
  for (double x : {0.1, 0.7, 1.3, 0.2, 0.9}) {
    ring.push(x);
    CHECK(one.update(x) == x);
    residue |= running.update(ring) != x;
  }
  CHECK(residue);
}

TEST_CASE("the EMA is seeded with the mean of the first n values") {
  ops::Ema ema;
  ema.set(3);
  CHECK(std::isnan(ema.update(1)));
  CHECK(std::isnan(ema.update(2)));
  CHECK(ema.update(3) == 2);
  CHECK(ema.update(6) == 0.5 * 6 + 0.5 * 2);
}

TEST_CASE("three-valued operators and the target rule") {
  CHECK(ops::and3(0, ops::NaN) == 0);
  CHECK(std::isnan(ops::and3(1, ops::NaN)));
  CHECK(ops::or3(ops::NaN, 1) == 1);
  CHECK(std::isnan(ops::or3(0, ops::NaN)));
  CHECK(std::isnan(ops::div(1, 0)));
  CHECK(std::isnan(ops::pick(ops::NaN, 1, 2)));
  ops::Target t;
  t.state(ops::NaN);
  CHECK(!t.stated());
  t.state(2e12);
  CHECK(!t.stated());
  t.state(-2.7);
  CHECK(t.stated());
  CHECK(t.shares() == -2);
}

TEST_CASE("the functions follow the undefined rules") {
  CHECK(ops::sign_of(-2.5) == -1);
  CHECK(ops::sign_of(0) == 0);
  CHECK(ops::sign_of(3) == 1);
  CHECK(std::isnan(ops::sign_of(ops::NaN)));
  CHECK(std::isnan(ops::log_of(0)));
  CHECK(std::isnan(ops::log_of(-1)));
  CHECK(ops::log_of(1) == 0);
  CHECK(std::isnan(ops::sqrt_of(-1)));
  CHECK(ops::round_of(2.5) == 3);
  CHECK(ops::round_of(-2.5) == -3);
  CHECK(ops::round_of(2.4999) == 2);
  CHECK(ops::floor_of(-0.5) == -1);
  CHECK(ops::whole("n", 3) == 3);
  CHECK_THROWS_AS(ops::whole("n", 2.5), std::invalid_argument);
  CHECK_THROWS_AS(ops::whole("n", 1e16), std::invalid_argument);
}

TEST_CASE("stdev and z-score: running sums agree with two passes, and a flat window has no z-score") {
  for (std::size_t n : {2u, 3u, 20u, 90u}) {
    const auto xs = walk(600, static_cast<unsigned>(n + 100));
    ops::Ring ring;
    ring.reset(n + 1);
    ops::Stdev sd;
    ops::Zscore z;
    sd.set(n);
    z.set(n);
    sd.reset();
    z.reset();
    for (std::size_t i = 0; i < xs.size(); ++i) {
      ring.push(xs[i]);
      const double a = sd.update(ring);
      const double b = sd.recompute(ring);
      const double c = z.update(ring);
      const double d = z.recompute(ring);
      if (i + 1 < n) {
        CHECK(std::isnan(a));
        CHECK(std::isnan(c));
      } else if (b > 0) {
        // A flat window is where running sums leave a residue; skipped.
        CHECK(std::fabs(a - b) <= 1e-8 * (1 + b));
        CHECK(std::fabs(c - d) <= 1e-7);
      }
    }
  }
  ops::Ring ring;
  ring.reset(4);
  ops::Zscore z;
  z.set(3);
  z.reset();
  for (int i = 0; i < 5; ++i) {
    ring.push(7);
    CHECK(std::isnan(z.update(ring)));
  }
}

TEST_CASE("a partial window sum is defined once its minimum count is in") {
  ops::TsSumMin sum;
  sum.set(4, 2);
  sum.reset();
  const std::vector<double> xs{1, 2, 3, 4, 5, 6};
  const std::vector<double> want{ops::NaN, 3, 6, 10, 14, 18};
  for (std::size_t i = 0; i < xs.size(); ++i) {
    CHECK(same(sum.update(xs[i]), want[i]));
  }
}

TEST_CASE("running least squares agree with the window solved again") {
  for (std::size_t n : {2u, 5u, 20u, 60u}) {
    const auto ys = walk(800, static_cast<unsigned>(n));
    const auto xs = walk(800, static_cast<unsigned>(n + 7));
    ops::OlsSlope slope;
    ops::OlsIntercept intercept;
    ops::OlsLlt<true> slope_llt;
    ops::OlsLlt<false> intercept_llt;
    slope.set(n);
    intercept.set(n);
    slope_llt.set(n);
    intercept_llt.set(n);
    slope.reset();
    intercept.reset();
    slope_llt.reset();
    intercept_llt.reset();
    for (std::size_t i = 0; i < ys.size(); ++i) {
      const double a = slope.update(ys[i], xs[i]);
      const double b = slope_llt.update(ys[i], xs[i]);
      const double c = intercept.update(ys[i], xs[i]);
      const double d = intercept_llt.update(ys[i], xs[i]);
      if (i + 1 < n) {
        CHECK(std::isnan(a));
        CHECK(std::isnan(b));
      } else if (!std::isnan(a)) {
        // A flat x window has no slope: NaN here, an arbitrary number from the singular solve.
        CHECK(std::fabs(a - b) <= 1e-6 * (1 + std::fabs(b)));
        CHECK(std::fabs(c - d) <= 1e-4 * (1 + std::fabs(d)));
      }
    }
  }
}

TEST_CASE("the Kalman hedge follows Chan's recursions") {
  // The recursions written with Eigen's matrices.
  const double delta = 0.0001;
  const double ve = 0.001;
  Eigen::Vector2d beta = Eigen::Vector2d::Zero();
  Eigen::Vector2d x = Eigen::Vector2d::Zero();
  const Eigen::Matrix2d vw = (delta / (1 - delta)) * Eigen::Matrix2d::Identity();
  Eigen::Matrix2d r = -vw;
  ops::KalmanHedge kf;
  kf.set(delta, ve);
  kf.restart();
  const auto ys = walk(1000, 3);
  const auto xs = walk(1000, 4);
  for (std::size_t i = 0; i < ys.size(); ++i) {
    const double yi = ys[i] / 1000;
    const double xi = xs[i] / 1000;
    x[0] = xi;
    x[1] = 1;
    r += vw;
    const double forecast = x.dot(beta);
    const double q = x.transpose() * r * x + ve;
    const Eigen::Vector2d k = r * x / q;
    const double e = yi - forecast;
    beta += k * e;
    r -= k * x.transpose() * r;
    kf.update(yi, xi);
    CHECK(std::fabs(kf.error() - e) <= 1e-9);
    CHECK(std::fabs(kf.sd() - std::sqrt(q)) <= 1e-9);
    CHECK(std::fabs(kf.beta() - beta[0]) <= 1e-9);
  }
}

TEST_CASE("skip windows: the values that exist in the last n slots, by every route") {
  std::mt19937 rng(7);
  for (std::size_t n : {1u, 2u, 5u, 20u, 90u}) {
    const auto xs = walk(700, static_cast<unsigned>(n) + 100);
    for (std::size_t min : {std::size_t{1}, n}) {
      ops::Ring ring;
      ring.reset(n + 1);
      ops::SmaSkip sma;
      ops::TsSumSkip sum;
      ops::StdevPopSkip pop;
      ops::StdevSkip sd;
      ops::ZscoreSkip z;
      for (auto* s : {static_cast<ops::SkipSums*>(&sma), static_cast<ops::SkipSums*>(&sum), static_cast<ops::SkipSums*>(&pop),
                      static_cast<ops::SkipSums*>(&sd), static_cast<ops::SkipSums*>(&z)}) {
        s->set(n);
        s->set_min(min);
        s->reset();
      }
      std::vector<double> seen;
      for (const double x0 : xs) {
        const double x = rng() % 5 == 0 ? ops::NaN : x0;
        seen.push_back(x);
        ring.push(x);
        const double a = sma.update(ring);
        const double t = sum.update(ring);
        const double p = pop.update(ring);
        const double s = sd.update(ring);
        const double zu = z.update(ring);
        // The window summed directly.
        double total = 0;
        std::size_t count = 0;
        if (seen.size() >= n) {
          for (std::size_t k = seen.size() - n; k < seen.size(); ++k) {
            if (!std::isnan(seen[k])) {
              total += seen[k];
              ++count;
            }
          }
        }
        const bool defined = seen.size() >= n && count >= min && count > 0;
        CHECK(std::isnan(a) == !defined);
        CHECK(same(a, sma.recompute(ring)));
        CHECK(same(t, sum.recompute(ring)));
        if (defined) {
          CHECK(a == total / static_cast<double>(count));
          CHECK(t == total);
          // On whole numbers the running sums are exact; the two passes agree to rounding.
          CHECK(std::fabs(p - pop.recompute(ring)) <= 1e-9 * (1 + p));
          CHECK(std::isnan(s) == (count < 2));
          if (count == 1) {
            CHECK(p == 0);
          }
          if (count >= 2 && s > 0 && !std::isnan(x)) {
            CHECK(std::fabs(zu - z.recompute(ring)) <= 1e-7);
          }
        }
        if (std::isnan(x)) {
          CHECK(std::isnan(zu));
        }
      }
    }
  }
}

// The reference's table, row by row: in each mode the generated code calls the method named here (the printer's
// tests check that), and each method computes the algorithm the table names, to the bit, on prices in cents.
TEST_CASE("each indicator's arithmetic is the algorithm the reference names") {
  const std::size_t n = 20;
  const auto xs = cents(400, 11);
  ops::Ring ring;
  ring.reset(n + 2);
  ops::Sma sma;
  ops::SmaExact sma_exact;
  ops::TsSum sum;
  ops::Rsi rsi;
  ops::Stdev sd;
  ops::StdevPop pop;
  ops::Zscore z;
  ops::Ema ema;
  for (auto* w : {static_cast<ops::Window*>(&sma), static_cast<ops::Window*>(&sum), static_cast<ops::Window*>(&rsi),
                  static_cast<ops::Window*>(&sd), static_cast<ops::Window*>(&pop), static_cast<ops::Window*>(&ema)}) {
    w->set(n);
  }
  sma_exact.set(n);
  z.set(n);
  // The running sums, written out: add the newest value, subtract the oldest.
  double s = 0;
  double q = 0;
  double up = 0;
  double down = 0;
  double e = 0;
  bool residue = false;
  for (std::size_t i = 0; i < xs.size(); ++i) {
    const double x = xs[i];
    ring.push(x);
    s += x;
    q += x * x;
    if (i >= 1) {
      const double d = x - xs[i - 1];
      (d > 0 ? up : down) += std::fabs(d);
    }
    if (i >= n) {
      const double old = xs[i - n];
      s -= old;
      q -= old * old;
    }
    if (i >= n + 1) {
      const double d = xs[i - n] - xs[i - n - 1];
      (d > 0 ? up : down) -= std::fabs(d);
    }
    // Summed again, oldest first, and two passes.
    double again = 0;
    for (std::size_t k = i + 1 - std::min(i + 1, n); k <= i; ++k) {
      again += xs[k];
    }
    const double mean = again / static_cast<double>(n);
    double squares = 0;
    for (std::size_t k = i + 1 - std::min(i + 1, n); k <= i; ++k) {
      squares += (xs[k] - mean) * (xs[k] - mean);
    }
    const double nd = static_cast<double>(n);
    const bool full = i + 1 >= n;
    // incremental: running sums (sma, ts_sum, rsi, stdev, stdev_pop, zscore).
    CHECK(same(sma.update(ring), full ? s / nd : ops::NaN));
    CHECK(same(sum.update(ring), full ? s : ops::NaN));
    CHECK(same(rsi.update(ring), i >= n ? (up + down == 0 ? 50 : 100 * up / (up + down)) : ops::NaN));
    CHECK(same(sd.update(ring), full ? std::sqrt((q - s * s / nd) / (nd - 1)) : ops::NaN));
    CHECK(same(pop.update(ring), full ? std::sqrt(std::max(0.0, (q - s * s / nd) / nd)) : ops::NaN));
    const double zd = std::sqrt((q - s * s / nd) / (nd - 1));
    CHECK(same(z.update(ring), full && zd > 0 ? (x - s / nd) / zd : ops::NaN));
    // naive: summed again (sma, ts_sum) and two passes (stdev, stdev_pop, zscore).
    CHECK(same(sma.recompute(ring), full ? mean : ops::NaN));
    CHECK(same(sum.recompute(ring), full ? again : ops::NaN));
    CHECK(same(sd.recompute(ring), full ? std::sqrt(squares / (nd - 1)) : ops::NaN));
    CHECK(same(pop.recompute(ring), full ? std::sqrt(squares / nd) : ops::NaN));
    // exact: no subtraction, so no residue: equal to a sum again up to the order of its additions.
    const double exact = sma_exact.update(x);
    if (full) {
      CHECK(std::fabs(exact - mean) <= 1e-12 * mean);
      residue |= s / nd != mean;
    }
    // ema: its recurrence, in every mode.
    if (i + 1 < n) {
      e += x;
    } else if (i + 1 == n) {
      e = (e + x) / nd;
    } else {
      const double alpha = 2.0 / (nd + 1.0);
      e = alpha * x + (1 - alpha) * e;
    }
    CHECK(same(ema.update(x), i + 1 >= n ? e : ops::NaN));
  }
  // Prices in cents leave running sums apart from sums again, which is what makes the modes differ at all.
  CHECK(residue);
}

// A day whose price equals its average to the cent: the comparison is decided by the arithmetic, as the
// reference says, and each mode decides it its own way.
TEST_CASE("a tie at decimal equality is decided by the mode's arithmetic") {
  ops::Ring ring;
  ring.reset(3);
  ops::Sma running;
  ops::Sma again;
  ops::SmaExact exact;
  running.set(2);
  again.set(2);
  exact.set(2);
  double incremental = ops::NaN;
  double naive = ops::NaN;
  double subtraction_free = ops::NaN;
  for (const double x : {0.1, 0.2, 0.3}) {
    ring.push(x);
    incremental = running.update(ring);
    naive = again.recompute(ring);
    subtraction_free = exact.update(x);
  }
  // The window is 0.20 and 0.30: in decimal its average is 0.25, and so is the price.
  const double price = 0.25;
  // Running sums keep 0.1's rounding residue after it left: the average comes out above 0.25.
  CHECK(incremental == 0.25000000000000006);
  CHECK(price < incremental);
  // Summed again, and without subtraction, the window holds no residue: no trade on `price < average`.
  CHECK(naive == 0.25);
  CHECK(subtraction_free == 0.25);
  CHECK(!(price < naive));
  CHECK(!(price < subtraction_free));
}

namespace {

// A stand-in for a compiled single-instrument strategy.
struct Slot {
  static constexpr bool reads_bar_history = false;
  atls::atl::InstrumentId instrument_;
  int bars_{0};
  template <class Ctx>
  void on_bar(Ctx&, const atls::atl::strategy::BarEvent&) {
    ++bars_;
  }
};

atls::atl::strategy::BarEvent bar_of(const char* symbol) {
  atls::atl::strategy::BarEvent bar;
  bar.instrument_ = atls::atl::InstrumentId{atls::atl::Country::KR, atls::atl::Venue::COMBINED, symbol};
  return bar;
}

} // namespace

TEST_CASE("a universe forwards each bar to the slot of its instrument") {
  int ctx = 0;
  atls::Universe<Slot> each;
  each.slots_ = {Slot{bar_of("A").instrument_}, Slot{bar_of("B").instrument_}};
  each.on_bar(ctx, bar_of("B"));
  each.on_bar(ctx, bar_of("B"));
  each.on_bar(ctx, bar_of("C"));
  CHECK(each.slots_[0].bars_ == 0);
  CHECK(each.slots_[1].bars_ == 2);
  atls::UniverseBars<Slot> slices;
  slices.slots_ = each.slots_;
  atls::atl::strategy::BarSlice slice;
  slice.bars_ = {bar_of("A"), bar_of("B"), bar_of("C")};
  slices.on_bars(ctx, slice);
  CHECK(slices.slots_[0].bars_ == 1);
  CHECK(slices.slots_[1].bars_ == 3);
  static_assert(!atls::Universe<Slot>::reads_bar_history);
}

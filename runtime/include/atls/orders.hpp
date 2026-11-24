#pragma once

#include "./expr.hpp"

#include <algotradelib/strategy>

#include <cmath>
#include <cstddef>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <utility>

/**
 * @file
 * @brief The target rule and how a target is priced: `Target`, `PricingRefusals` and `OrderPricing`.
 */

namespace atls {
namespace ops {

/**
 * @brief What a handler states this bar: the last defined target, as whole shares truncated toward zero.
 *   An undefined, infinite or larger than 1e12 value states nothing.
 */
class Target {
public:
  /** @brief The largest number of shares a target may state. */
  static constexpr double MAX_SHARES = 1e12;

  /**
   * @brief States a real-valued target.
   * @param value The shares.
   */
  void state(double value) {
    if (std::isfinite(value) && std::fabs(value) <= MAX_SHARES) {
      stated_ = true;
      shares_ = static_cast<std::int64_t>(std::trunc(value));
    }
  }

  /**
   * @brief States a whole number of shares.
   * @param value The shares.
   */
  void state(std::int64_t value) {
    if (value <= static_cast<std::int64_t>(MAX_SHARES) && value >= -static_cast<std::int64_t>(MAX_SHARES)) {
      stated_ = true;
      shares_ = value;
    }
  }

  /** @brief Whether anything was stated. */
  bool stated() const {
    return stated_;
  }
  /** @brief The shares stated. */
  std::int64_t shares() const {
    return shares_;
  }

private:
  bool stated_{false};
  std::int64_t shares_{0};
};

/** @brief How many stated targets limit pricing did not submit, by reason. */
struct PricingRefusals {
  /** @brief The price of the event that ran the handler was missing or not positive. */
  std::size_t no_basis_price_{0};
  /** @brief The instrument had no reference data, or its price grid is unknown. */
  std::size_t no_price_grid_{0};
  /** @brief The limit's arithmetic, or its rounding onto the grid, did not fit a decimal. */
  std::size_t price_not_representable_{0};
  /** @brief The limit, rounded onto the grid, was not positive. */
  std::size_t price_not_positive_{0};
  /** @brief The net position, summed over every holding row, did not fit a decimal. */
  std::size_t net_not_representable_{0};

  /**
   * @brief Every refusal.
   * @return The sum of the counts.
   */
  std::size_t total() const {
    return no_basis_price_ + no_price_grid_ + price_not_representable_ + price_not_positive_ + net_not_representable_;
  }
};

/**
 * @brief How a stated target becomes an order: a market order (the default), or a limit priced from the price of the
 *   event that ran the handler.
 *
 * The limit is `basis × (1 + offset)` for a buy and `basis × (1 − offset)` for a sell, computed exactly, then put on
 * the instrument's tick grid on the safe side (`Price::rounded`: a buy down, a sell up). The side compares the target
 * with the net position summed over every holding row. A target equal to the position is submitted without a limit,
 * so that the runner still cancels unwanted working orders. A target that cannot be priced is not submitted and is
 * counted by reason (`refusals()`): a requested limit never becomes a market order.
 */
class OrderPricing {
public:
  /**
   * @brief Sets the pricing.
   * @param type `OrderType::MARKET` or `OrderType::LIMIT`.
   * @param offset The limit's offset, at least 0 and below 0.3; 0 with `MARKET`.
   * @throws std::invalid_argument For another type, an offset outside its range, an offset with `MARKET`, or an
   *   offset whose `1 ± offset` does not fit a decimal.
   */
  void set(atl::OrderType type, const atl::Decimal64& offset) {
    if (type != atl::OrderType::MARKET && type != atl::OrderType::LIMIT) {
      throw std::invalid_argument("order pricing: the order type is market or limit");
    }
    const atl::Decimal64 zero(0, 0);
    if (atl::decimal_less(offset, zero) || !atl::decimal_less(offset, atl::Decimal64(3, 1))) {
      throw std::invalid_argument("order pricing: the offset is at least 0 and below 0.3, not " + offset.str());
    }
    if (type == atl::OrderType::MARKET && offset.coefficient() != 0) {
      throw std::invalid_argument("order pricing: an offset prices a limit, not a market order");
    }
    const atl::Decimal64 one(1, 0);
    const auto buy = atl::decimal_add(one, offset);
    const auto sell = atl::decimal_subtract(one, offset);
    if (!buy || !sell) {
      throw std::invalid_argument(
        "order pricing: 1 plus or minus the offset " + offset.str() + " does not fit a decimal"
      );
    }
    limit_ = type == atl::OrderType::LIMIT;
    buy_ = *buy;
    sell_ = *sell;
  }

  /**
   * @brief Whether targets are priced as limits.
   * @return `true` after `set` with `OrderType::LIMIT`.
   */
  bool limit() const {
    return limit_;
  }

  /**
   * @brief Prices a target as a limit, reading the net position.
   * @param ctx The runner's context: its portfolio and the instrument's reference data.
   * @param order The target; its `limit_` is set when it is priced.
   * @param basis The price of the event that ran the handler.
   * @return Whether to submit the target; `false` after counting a refusal.
   */
  template <class Ctx>
  bool price(Ctx& ctx, atl::strategy::TargetPosition& order, const atl::Decimal64& basis) {
    const auto net = ctx.portfolio().net_quantity_of(order.instrument_);
    if (!net) {
      ++refusals_.net_not_representable_;
      return false;
    }
    return price(ctx, order, basis, *net);
  }

  /**
   * @brief Prices a target as a limit, against a net position the handler already read.
   * @param ctx The runner's context: the instrument's reference data.
   * @param order The target; its `limit_` is set when it is priced.
   * @param basis The price of the event that ran the handler.
   * @param net The net position, summed over every holding row.
   * @return Whether to submit the target; `false` after counting a refusal.
   */
  template <class Ctx>
  bool price(Ctx& ctx, atl::strategy::TargetPosition& order, const atl::Decimal64& basis, const atl::Decimal64& net) {
    const bool buy = atl::decimal_less(net, order.target_quantity_);
    if (!buy && !atl::decimal_less(order.target_quantity_, net)) {
      return true;
    }
    if (basis.coefficient() <= 0) {
      ++refusals_.no_basis_price_;
      return false;
    }
    const auto instrument = ctx.instrument(order.instrument_);
    if (!instrument || !instrument->price_rule_.grid_known()) {
      ++refusals_.no_price_grid_;
      return false;
    }
    const auto wanted = atl::decimal_multiply(basis, buy ? buy_ : sell_);
    if (!wanted) {
      ++refusals_.price_not_representable_;
      return false;
    }
    auto limit =
      atl::strategy::Price::rounded(*wanted, instrument->price_rule_, buy ? atl::OrderSide::BUY : atl::OrderSide::SELL);
    if (!limit) {
      ++refusals_.price_not_representable_;
      return false;
    }
    if (limit->value().coefficient() <= 0) {
      ++refusals_.price_not_positive_;
      return false;
    }
    order.limit_ = std::move(limit);
    return true;
  }

  /**
   * @brief The targets not submitted, by reason, since the strategy was made.
   * @return The counts.
   */
  const PricingRefusals& refusals() const {
    return refusals_;
  }

private:
  bool limit_{false};
  atl::Decimal64 buy_{1, 0};
  atl::Decimal64 sell_{1, 0};
  PricingRefusals refusals_;
};

} // namespace ops
} // namespace atls

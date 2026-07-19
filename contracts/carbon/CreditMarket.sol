// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "./CreditRegistry.sol";

/// @notice Simple fixed-price orderbook for AgriTrust carbon credits.
contract CreditMarket {
    struct Order {
        address seller;
        uint256 tokenId;
        uint256 amountRemaining;
        uint256 unitPriceWei;
        bool active;
    }

    CreditRegistry public immutable registry;
    uint256 public nextOrderId = 1;
    mapping(uint256 => Order) public orders;

    event OrderListed(uint256 indexed orderId, address indexed seller, uint256 indexed tokenId, uint256 amount, uint256 unitPriceWei);
    event OrderFilled(uint256 indexed orderId, address indexed buyer, uint256 amount, uint256 totalPrice);
    event OrderCancelled(uint256 indexed orderId);

    error InvalidOrder();
    error OnlySeller();
    error IncorrectPayment();

    constructor(CreditRegistry registry_) {
        registry = registry_;
    }

    function list(uint256 tokenId, uint256 amount, uint256 unitPriceWei) external returns (uint256 orderId) {
        orderId = nextOrderId++;
        orders[orderId] = Order(msg.sender, tokenId, amount, unitPriceWei, true);
        emit OrderListed(orderId, msg.sender, tokenId, amount, unitPriceWei);
    }

    function buy(uint256 orderId, uint256 amount) external payable {
        Order storage order = orders[orderId];
        if (!order.active || amount == 0 || amount > order.amountRemaining) revert InvalidOrder();
        uint256 totalPrice = amount * order.unitPriceWei;
        if (msg.value != totalPrice) revert IncorrectPayment();
        order.amountRemaining -= amount;
        if (order.amountRemaining == 0) order.active = false;
        registry.safeTransferFrom(order.seller, msg.sender, order.tokenId, amount, "");
        payable(order.seller).transfer(totalPrice);
        emit OrderFilled(orderId, msg.sender, amount, totalPrice);
    }

    function cancel(uint256 orderId) external {
        Order storage order = orders[orderId];
        if (!order.active) revert InvalidOrder();
        if (msg.sender != order.seller) revert OnlySeller();
        order.active = false;
        emit OrderCancelled(orderId);
    }
}

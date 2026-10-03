# frozen_string_literal: true

module Inventory
  # A stocked item with a price in cents.
  class Item
    include Comparable

    attr_reader :sku, :name, :price_cents
    attr_accessor :quantity

    def initialize(sku:, name:, price_cents:, quantity: 0)
      @sku = sku
      @name = name
      @price_cents = price_cents
      @quantity = quantity
    end

    def <=>(other)
      sku <=> other.sku
    end

    def total_cents
      price_cents * quantity
    end

    def in_stock?
      quantity.positive?
    end

    def label
      "#{name} (#{sku}) x#{quantity}"
    end

    def to_h
      { sku: sku, name: name, price_cents: price_cents, quantity: quantity }
    end

    def self.from_h(attributes)
      new(
        sku: attributes.fetch(:sku),
        name: attributes.fetch(:name),
        price_cents: attributes.fetch(:price_cents),
        quantity: attributes.fetch(:quantity, 0)
      )
    end
  end
end

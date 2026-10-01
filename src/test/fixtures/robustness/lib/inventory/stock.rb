# frozen_string_literal: true

require_relative "item"

module Inventory
  class OutOfStock < StandardError
    def initialize(sku)
      super("out of stock: #{sku}")
    end
  end

  # A collection of items keyed by SKU.
  class Stock
    include Enumerable

    def initialize
      @items = {}
    end

    def add(item)
      existing = @items[item.sku]
      if existing
        existing.quantity += item.quantity
      else
        @items[item.sku] = item
      end
      self
    end

    def take(sku, count = 1)
      item = @items.fetch(sku) { raise OutOfStock, sku }
      raise OutOfStock, sku if item.quantity < count

      item.quantity -= count
      item
    end

    def each(&block)
      @items.values.sort.each(&block)
    end

    def value_cents
      sum(&:total_cents)
    end

    def low(threshold: 3)
      select { |item| item.quantity < threshold }.map(&:sku)
    end

    def grouped_by_price
      group_by do |item|
        case item.price_cents
        when 0...1_000 then :cheap
        when 1_000...10_000 then :regular
        else :premium
        end
      end
    end

    def find_sku(sku)
      @items[sku]&.label
    end
  end
end

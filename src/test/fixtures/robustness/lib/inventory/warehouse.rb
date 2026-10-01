# frozen_string_literal: true

require_relative "stock"
require_relative "events"
require_relative "parsing"

module Inventory
  # Coordinates receiving and shipping against one stock.
  class Warehouse
    include Events

    emits :received, :shipped

    attr_reader :stock

    def initialize(name)
      @name = name
      @stock = Stock.new
      @log = []
    end

    def receive(text)
      rows = Parsing.parse(text)
      rows.each do |row|
        item = Item.new(sku: row.sku, name: row.name, price_cents: row.price_cents, quantity: row.quantity)
        stock.add(item)
        emit(:received, item)
      end
      rows.size
    end

    def ship(sku, count: 1)
      item = stock.take(sku, count)
      @log.push([:shipped, sku, count])
      emit(:shipped, item, count: count)
      item
    rescue OutOfStock => error
      @log.push([:failed, sku, error.message])
      nil
    ensure
      @log.shift while @log.size > 100
    end

    def history
      @log.map { |kind, sku, detail| "#{kind}: #{sku} (#{detail})" }
    end

    def summary
      Report.new(stock, title: @name).render
    end

    def counts
      totals = Hash.new(0)
      stock.each { |item| totals[item.in_stock? ? :available : :empty] += 1 }
      totals
    end

    def each_batch(size = 2)
      return enum_for(:each_batch, size) unless block_given?

      stock.each_slice(size) { |batch| yield batch }
    end

    def busy?
      !@log.empty? && @log.last.first == :shipped
    end

    def to_s = "#<Warehouse #{@name}>"

    private

    def reset!
      @log.clear
      @stock = Stock.new
    end
  end
end

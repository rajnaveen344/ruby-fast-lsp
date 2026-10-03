# frozen_string_literal: true

module Inventory
  module Formatting
    CURRENCY = "USD"

    def money(cents)
      format("%<units>d.%<fraction>02d %<currency>s",
             units: cents / 100, fraction: cents % 100, currency: CURRENCY)
    end
  end

  # Renders a plain-text summary of a stock.
  class Report
    include Formatting

    HEADER = <<~TEXT
      Inventory report
      ================
    TEXT

    def initialize(stock, title: "Summary")
      @stock = stock
      @title = title
    end

    def render
      lines = [HEADER, @title]
      @stock.each_with_index do |item, index|
        lines << "#{index + 1}. #{item.label} - #{money(item.total_cents)}"
      end
      lines << "Total: #{money(@stock.value_cents)}"
      lines.join("\n")
    end

    def tiers
      @stock.grouped_by_price.transform_values(&:count)
    end
  end
end

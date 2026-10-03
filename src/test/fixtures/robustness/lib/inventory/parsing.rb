# frozen_string_literal: true

module Inventory
  # Parses "sku,name,price,quantity" rows.
  module Parsing
    Row = Struct.new(:sku, :name, :price_cents, :quantity, keyword_init: true)

    module_function

    def parse_line(line)
      sku, name, price, quantity = line.strip.split(",", 4)
      return nil if sku.nil? || sku.empty?

      Row.new(sku: sku, name: name.to_s, price_cents: Integer(price || 0), quantity: Integer(quantity || 0))
    rescue ArgumentError => error
      warn("skipping #{line.inspect}: #{error.message}")
      nil
    end

    def parse(text)
      text.each_line.filter_map { |line| parse_line(line) }
    end

    def classify(value)
      case value
      in Integer | Float => number if number.negative?
        :negative
      in Integer | Float
        :number
      in String => text if text.match?(/\A\d+\z/)
        :numeric_text
      in { sku: String => sku }
        "row #{sku}"
      in [first, *rest]
        [first, rest.size]
      else
        :other
      end
    end

    def checksum(text)
      text.each_char.reduce(0) { |sum, char| (sum * 31 + char.ord) % 65_521 }
    end

    def glyphs
      ["café", "naïve", "📦 box", "日本"]
    end
  end
end

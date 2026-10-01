# frozen_string_literal: true

require_relative "lib/inventory/warehouse"
require_relative "lib/inventory/report"

warehouse = Inventory::Warehouse.new("Main")
warehouse.on_received { |item| puts "received #{item.label}" }
warehouse.on_shipped { |item, count:| puts "shipped #{count} of #{item.sku}" }

received = warehouse.receive(<<~CSV)
  A1,Bolt,25,40
  B2,Bracket,1250,2
  C3,Drill,15999,1
CSV

warehouse.ship("A1", count: 5)
warehouse.ship("Z9")

puts received
puts warehouse.summary
puts warehouse.history.join("\n")
puts warehouse.counts.fetch(:available, 0)
warehouse.each_batch { |batch| puts batch.map(&:sku).inspect }

low = warehouse.stock.low(threshold: 5)
first, *others = low
puts [first, others].inspect

square = ->(value) { value * value }
puts [1, 2, 3].map(&square).sum
puts Inventory::Parsing.classify({ sku: "A1" })
puts Inventory::Parsing.glyphs.map(&:length).inspect
puts Inventory::Parsing.checksum(warehouse.to_s)

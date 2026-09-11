module Vocabulary
  KEYS = [:title, :author].freeze
end

names = Vocabulary::KEYS.map do |key|
  key.to_s
end

puts names

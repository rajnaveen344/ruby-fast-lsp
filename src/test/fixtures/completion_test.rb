class Product
  attr_accessor :name, :price, :description

  def initialize(name, price, description = nil)
    @name = name
    @price = price
    @description = description
  end

  def discount(percent)
    @price * (1 - percent / 100.0)
  end

  def to_s
    "#{@name}: $#{@price}"
  end
end

product = Product.new("Book", 29.99, "A great book")
product.

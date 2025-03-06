module OrderProcessing
  class Order
    include Validatable

    attr_reader :id, :user, :items, :total

    def initialize(user, items = [])
      @id = generate_order_id
      @user = user
      @items = items
      @total = calculate_total
    end

    def add_item(product, quantity = 1)
      @items << OrderItem.new(product, quantity)
      recalculate_total
    end

    private

    def calculate_total
      @items.sum(&:subtotal)
    end

    def generate_order_id
      "ORD-#{Time.now.to_i}-#{rand(1000)}"
    end
  end
end

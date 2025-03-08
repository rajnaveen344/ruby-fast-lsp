class BankAccount
  attr_reader :balance

  def initialize(initial_balance = 0)
    @balance = initial_balance
  end

  def deposit(amount)
    @balance += amount
    log_transaction("deposit", amount)
  end

  def withdraw(amount)
    if amount <= @balance
      @balance -= amount
      log_transaction("withdraw", amount)
      true
    else
      false
    end
  end

  private

  def log_transaction(type, amount)
    puts "#{type} transaction: #{amount} - new balance: #{@balance}"
  end
end

account = BankAccount.new(100)
account.deposit(50)
account.withdraw(25)
puts "Final balance: #{account.balance}"

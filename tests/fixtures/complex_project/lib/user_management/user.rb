# A complex User class with various Ruby features
module UserManagement
  class User
    include Authentication
    extend ActiveSupport

    attr_accessor :first_name, :last_name, :email
    attr_reader :created_at

    @@user_count = 0

    def initialize(attributes = {})
      @first_name = attributes[:first_name]
      @last_name = attributes[:last_name]
      @email = attributes[:email]
      @created_at = Time.now
      @@user_count += 1
    end

    def full_name
      "#{@first_name} #{@last_name}"
    end

    def self.count
      @@user_count
    end

    private

    def validate_email
      @email.match?(/\A[\w+\-.]+@[a-z\d\-]+(\.[a-z\d\-]+)*\.[a-z]+\z/i)
    end
  end
end

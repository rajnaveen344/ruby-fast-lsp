# Test fixture for method call resolution
# Tests Milestone 5: Method Call Type Resolution

class User
  # @return [String]
  def name
    @name
  end

  # @return [Integer]
  def age
    @age
  end

  # @return [Boolean]
  def active?
    @active
  end

  # @return [User]
  def self.find(id)
    # Returns a User instance
  end

  # @return [Array<User>]
  def self.all
    # Returns array of users
  end
end

class Profile
  # @return [String]
  def bio
    @bio
  end

  # @return [User]
  def user
    @user
  end
end

# Test: User.new should return User instance
user = User.new

# Test: user.name should return String
name = user.name

# Test: User.find(1) should return User
found_user = User.find(1)

# Test: chained call user.profile.bio
# (requires profile method on User)

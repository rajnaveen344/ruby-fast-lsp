# Test file that uses a class from another file
# This simulates the real-world scenario where User is defined elsewhere

# Assuming User class is defined in another file with:
# class User
#   # @return [String]
#   def name
#     @name
#   end
# end

# Test: User.new should return User instance
user = User.new

# Test: user.name should return String (if User class is indexed)
name = user.name

class Types
  def read
    @missing
    @value = dynamic_value
    @value
  end
end

class User
  def profile
    dynamic_profile
  end
end

User.new.profile
dynamic_user.fetch

class Choice
  def value(flag)
    if flag
      "text"
    else
      1
    end
  end
end

Choice.new.value(true).length

# Test fixture for return type inference without YARD documentation

class ReturnTypeInference
  # Simple literal returns
  def string_return
    "hello"
  end

  def integer_return
    42
  end

  def float_return
    3.14
  end

  def symbol_return
    :ok
  end

  def true_return
    true
  end

  def false_return
    false
  end

  def nil_return
    nil
  end

  def array_return
    [1, 2, 3]
  end

  def hash_return
    { name: "test", value: 42 }
  end

  # Early returns
  def early_return_nil(value)
    return nil if value.nil?
    "processed"
  end

  def early_return_different_type(value)
    return "error" if value < 0
    42
  end

  # Conditional returns
  def if_else_same_type(flag)
    if flag
      "yes"
    else
      "no"
    end
  end

  def if_else_different_types(flag)
    if flag
      "yes"
    else
      42
    end
  end

  def if_without_else(flag)
    if flag
      "hello"
    end
  end

  def case_when_same_type(num)
    case num
    when 1 then "one"
    when 2 then "two"
    else "other"
    end
  end

  def case_when_different_types(type)
    case type
    when :string then "hello"
    when :number then 42
    else nil
    end
  end

  # Multiple statements
  def multiple_statements
    x = 1
    y = 2
    "result"
  end

  # Interpolated strings
  def interpolated_string(name)
    "Hello, #{name}!"
  end

  # Guard clauses
  def guard_clause(id)
    return unless id
    "found"
  end

  # Begin/rescue
  def begin_rescue_same_type
    begin
      "success"
    rescue
      "error"
    end
  end

  def begin_rescue_different_types
    begin
      42
    rescue
      nil
    end
  end

  # Method with YARD that should take precedence
  # @return [String] The greeting
  def with_yard_return
    42  # Return type should be String from YARD, not Integer
  end
end

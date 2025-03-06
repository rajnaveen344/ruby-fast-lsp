def test_method(param1, param2 = "default"); yield param1 if block_given?; param1 + param2; end; test_method("hello", "world") { |x| puts x }

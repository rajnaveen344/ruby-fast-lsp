# frozen_string_literal: true

# A \Complex object houses a pair of values,
# given when the object is created as either <i>rectangular coordinates</i>
# or <i>polar coordinates</i>.
#
# == Rectangular Coordinates
#
# The rectangular coordinates of a complex number
# are called the _real_ and _imaginary_ parts;
# see {Complex number definition}[https://en.wikipedia.org/wiki/Complex_number#Definition_and_basic_operations].
#
# You can create a \Complex object from rectangular coordinates with:
#
# - A {complex literal}[rdoc-ref:syntax/literals.rdoc@Complex+Literals].
# - \Method Complex.rect.
# - \Method Kernel#Complex, either with numeric arguments or with certain string arguments.
# - \Method String#to_c, for certain strings.
#
# Note that each of the stored parts may be a an instance one of the classes
# Complex, Float, Integer, or Rational;
# they may be retrieved:
#
# - Separately, with methods Complex#real and Complex#imaginary.
# - Together, with method Complex#rect.
#
# The corresponding (computed) polar values may be retrieved:
#
# - Separately, with methods Complex#abs and Complex#arg.
# - Together, with method Complex#polar.
#
# == Polar Coordinates
#
# The polar coordinates of a complex number
# are called the _absolute_ and _argument_ parts;
# see {Complex polar plane}[https://en.wikipedia.org/wiki/Complex_number#Polar_form].
#
# In this class, the argument part
# in expressed {radians}[https://en.wikipedia.org/wiki/Radian]
# (not {degrees}[https://en.wikipedia.org/wiki/Degree_(angle)]).
#
# You can create a \Complex object from polar coordinates with:
#
# - \Method Complex.polar.
# - \Method Kernel#Complex, with certain string arguments.
# - \Method String#to_c, for certain strings.
#
# Note that each of the stored parts may be a an instance one of the classes
# Complex, Float, Integer, or Rational;
# they may be retrieved:
#
# - Separately, with methods Complex#abs and Complex#arg.
# - Together, with method Complex#polar.
#
# The corresponding (computed) rectangular values may be retrieved:
#
# - Separately, with methods Complex#real and Complex#imag.
# - Together, with method Complex#rect.
#
# == What's Here
#
# First, what's elsewhere:
#
# - \Class \Complex inherits (directly or indirectly)
#   from classes {Numeric}[rdoc-ref:Numeric@What-27s+Here]
#   and {Object}[rdoc-ref:Object@What-27s+Here].
# - Includes (indirectly) module {Comparable}[rdoc-ref:Comparable@What-27s+Here].
#
# Here, class \Complex has methods for:
#
# === Creating \Complex Objects
#
# - ::polar: Returns a new \Complex object based on given polar coordinates.
# - ::rect (and its alias ::rectangular):
#   Returns a new \Complex object based on given rectangular coordinates.
#
# === Querying
#
# - #abs (and its alias #magnitude): Returns the absolute value for +self+.
# - #arg (and its aliases #angle and #phase):
#   Returns the argument (angle) for +self+ in radians.
# - #denominator: Returns the denominator of +self+.
# - #finite?: Returns whether both +self.real+ and +self.image+ are finite.
# - #hash: Returns the integer hash value for +self+.
# - #imag (and its alias #imaginary): Returns the imaginary value for +self+.
# - #infinite?: Returns whether +self.real+ or +self.image+ is infinite.
# - #numerator: Returns the numerator of +self+.
# - #polar: Returns the array <tt>[self.abs, self.arg]</tt>.
# - #inspect: Returns a string representation of +self+.
# - #real: Returns the real value for +self+.
# - #real?: Returns +false+; for compatibility with Numeric#real?.
# - #rect (and its alias #rectangular):
#   Returns the array <tt>[self.real, self.imag]</tt>.
#
# === Comparing
#
# - #<=>: Returns whether +self+ is less than, equal to, or greater than the given argument.
# - #==: Returns whether +self+ is equal to the given argument.
#
# === Converting
#
# - #rationalize: Returns a Rational object whose value is exactly
#   or approximately equivalent to that of <tt>self.real</tt>.
# - #to_c: Returns +self+.
# - #to_d: Returns the value as a BigDecimal object.
# - #to_f: Returns the value of <tt>self.real</tt> as a Float, if possible.
# - #to_i: Returns the value of <tt>self.real</tt> as an Integer, if possible.
# - #to_r: Returns the value of <tt>self.real</tt> as a Rational, if possible.
# - #to_s: Returns a string representation of +self+.
#
# === Performing Complex Arithmetic
#
# - #*: Returns the product of +self+ and the given numeric.
# - #**: Returns +self+ raised to power of the given numeric.
# - #+: Returns the sum of +self+ and the given numeric.
# - #-: Returns the difference of +self+ and the given numeric.
# - #-@: Returns the negation of +self+.
# - #/: Returns the quotient of +self+ and the given numeric.
# - #abs2: Returns square of the absolute value (magnitude) for +self+.
# - #conj (and its alias #conjugate): Returns the conjugate of +self+.
# - #fdiv: Returns <tt>Complex.rect(self.real/numeric, self.imag/numeric)</tt>.
#
# === Working with JSON
#
# - ::json_create: Returns a new \Complex object,
#   deserialized from the given serialized hash.
# - #as_json: Returns a serialized hash constructed from +self+.
# - #to_json: Returns a JSON string representing +self+.
#
# These methods are provided by the {JSON gem}[https://github.com/ruby/json]. To make these methods available:
#
#   require 'json/add/complex'
class Complex < Numeric
  # Equivalent
  # to <tt>Complex.rect(0, 1)</tt>:
  #
  #   Complex::I # => (0+1i)
  I = _

  # Returns a new \Complex object formed from the arguments,
  # each of which must be an instance of Numeric,
  # or an instance of one of its subclasses:
  # \Complex, Float, Integer, Rational.
  # Argument +arg+ is given in radians;
  # see {Polar Coordinates}[rdoc-ref:Complex@Polar+Coordinates]:
  #
  #   Complex.polar(3)        # => (3+0i)
  #   Complex.polar(3, 2.0)   # => (-1.2484405096414273+2.727892280477045i)
  #   Complex.polar(-3, -2.0) # => (1.2484405096414273+2.727892280477045i)
  def self.polar(abs, arg = 0) end

  # Returns a new \Complex object formed from the arguments,
  # each of which must be an instance of Numeric,
  # or an instance of one of its subclasses:
  # \Complex, Float, Integer, Rational;
  # see {Rectangular Coordinates}[rdoc-ref:Complex@Rectangular+Coordinates]:
  #
  #   Complex.rect(3)             # => (3+0i)
  #   Complex.rect(3, Math::PI)   # => (3+3.141592653589793i)
  #   Complex.rect(-3, -Math::PI) # => (-3-3.141592653589793i)
  #
  # \Complex.rectangular is an alias for \Complex.rect.
  def self.rect(real, imag = 0) end

  # Returns a new \Complex object formed from the arguments,
  # each of which must be an instance of Numeric,
  # or an instance of one of its subclasses:
  # \Complex, Float, Integer, Rational;
  # see {Rectangular Coordinates}[rdoc-ref:Complex@Rectangular+Coordinates]:
  #
  #   Complex.rect(3)             # => (3+0i)
  #   Complex.rect(3, Math::PI)   # => (3+3.141592653589793i)
  #   Complex.rect(-3, -Math::PI) # => (-3-3.141592653589793i)
  #
  # \Complex.rectangular is an alias for \Complex.rect.
  def self.rectangular(p1, p2 = v2) end

  # Returns the product of +self+ and +numeric+:
  #
  #   Complex.rect(2, 3)  * Complex.rect(2, 3)  # => (-5+12i)
  #   Complex.rect(900)   * Complex.rect(1)     # => (900+0i)
  #   Complex.rect(-2, 9) * Complex.rect(-9, 2) # => (0-85i)
  #   Complex.rect(9, 8)  * 4                   # => (36+32i)
  #   Complex.rect(20, 9) * 9.8                 # => (196.0+88.2i)
  def *(other) end

  # Returns +self+ raised to power +numeric+:
  #
  #   Complex.rect(0, 1) ** 2            # => (-1+0i)
  #   Complex.rect(-8) ** Rational(1, 3) # => (1.0000000000000002+1.7320508075688772i)
  def **(other) end

  # Returns the sum of +self+ and +numeric+:
  #
  #   Complex.rect(2, 3)  + Complex.rect(2, 3)  # => (4+6i)
  #   Complex.rect(900)   + Complex.rect(1)     # => (901+0i)
  #   Complex.rect(-2, 9) + Complex.rect(-9, 2) # => (-11+11i)
  #   Complex.rect(9, 8)  + 4                   # => (13+8i)
  #   Complex.rect(20, 9) + 9.8                 # => (29.8+9i)
  def +(other) end

  # Returns the difference of +self+ and +numeric+:
  #
  #   Complex.rect(2, 3)  - Complex.rect(2, 3)  # => (0+0i)
  #   Complex.rect(900)   - Complex.rect(1)     # => (899+0i)
  #   Complex.rect(-2, 9) - Complex.rect(-9, 2) # => (7+7i)
  #   Complex.rect(9, 8)  - 4                   # => (5+8i)
  #   Complex.rect(20, 9) - 9.8                 # => (10.2+9i)
  def -(other) end

  # Returns the negation of +self+, which is the negation of each of its parts:
  #
  #   -Complex.rect(1, 2)   # => (-1-2i)
  #   -Complex.rect(-1, -2) # => (1+2i)
  def -@; end

  # Returns the quotient of +self+ and +numeric+:
  #
  #   Complex.rect(2, 3)  / Complex.rect(2, 3)  # => (1+0i)
  #   Complex.rect(900)   / Complex.rect(1)     # => (900+0i)
  #   Complex.rect(-2, 9) / Complex.rect(-9, 2) # => ((36/85)-(77/85)*i)
  #   Complex.rect(9, 8)  / 4                   # => ((9/4)+2i)
  #   Complex.rect(20, 9) / 9.8                 # => (2.0408163265306123+0.9183673469387754i)
  def /(other) end

  # Returns:
  #
  # - <tt>self.real <=> object.real</tt> if both of the following are true:
  #
  #   - <tt>self.imag == 0</tt>.
  #   - <tt>object.imag == 0</tt>. # Always true if object is numeric but not complex.
  #
  # - +nil+ otherwise.
  #
  # Examples:
  #
  #   Complex.rect(2) <=> 3                  # => -1
  #   Complex.rect(2) <=> 2                  # => 0
  #   Complex.rect(2) <=> 1                  # => 1
  #   Complex.rect(2, 1) <=> 1               # => nil # self.imag not zero.
  #   Complex.rect(1) <=> Complex.rect(1, 1) # => nil # object.imag not zero.
  #   Complex.rect(1) <=> 'Foo'              # => nil # object.imag not defined.
  def <=>(other) end

  # Returns +true+ if <tt>self.real == object.real</tt>
  # and <tt>self.imag == object.imag</tt>:
  #
  #   Complex.rect(2, 3)  == Complex.rect(2.0, 3.0) # => true
  def ==(other) end

  # Returns the absolute value (magnitude) for +self+;
  # see {polar coordinates}[rdoc-ref:Complex@Polar+Coordinates]:
  #
  #   Complex.polar(-1, 0).abs # => 1.0
  #
  # If +self+ was created with
  # {rectangular coordinates}[rdoc-ref:Complex@Rectangular+Coordinates], the returned value
  # is computed, and may be inexact:
  #
  #   Complex.rectangular(1, 1).abs # => 1.4142135623730951 # The square root of 2.
  def abs; end
  alias magnitude abs

  # Returns square of the absolute value (magnitude) for +self+;
  # see {polar coordinates}[rdoc-ref:Complex@Polar+Coordinates]:
  #
  #   Complex.polar(2, 2).abs2 # => 4.0
  #
  # If +self+ was created with
  # {rectangular coordinates}[rdoc-ref:Complex@Rectangular+Coordinates], the returned value
  # is computed, and may be inexact:
  #
  #   Complex.rectangular(1.0/3, 1.0/3).abs2 # => 0.2222222222222222
  def abs2; end

  # Returns the argument (angle) for +self+ in radians;
  # see {polar coordinates}[rdoc-ref:Complex@Polar+Coordinates]:
  #
  #   Complex.polar(3, Math::PI/2).arg  # => 1.57079632679489660
  #
  # If +self+ was created with
  # {rectangular coordinates}[rdoc-ref:Complex@Rectangular+Coordinates], the returned value
  # is computed, and may be inexact:
  #
  #   Complex.polar(1, 1.0/3).arg # => 0.33333333333333326
  def arg; end
  alias angle arg
  alias phase arg

  # Returns the conjugate of +self+, <tt>Complex.rect(self.imag, self.real)</tt>:
  #
  #   Complex.rect(1, 2).conj # => (1-2i)
  def conjugate; end
  alias conj conjugate

  # Returns the denominator of +self+, which is
  # the {least common multiple}[https://en.wikipedia.org/wiki/Least_common_multiple]
  # of <tt>self.real.denominator</tt> and <tt>self.imag.denominator</tt>:
  #
  #   Complex.rect(Rational(1, 2), Rational(2, 3)).denominator # => 6
  #
  # Note that <tt>n.denominator</tt> of a non-rational numeric is +1+.
  #
  # Related: Complex#numerator.
  def denominator; end

  # Returns <tt>Complex.rect(self.real/numeric, self.imag/numeric)</tt>:
  #
  #   Complex.rect(11, 22).fdiv(3) # => (3.6666666666666665+7.333333333333333i)
  def fdiv(numeric) end

  # Returns +true+ if both <tt>self.real.finite?</tt> and <tt>self.imag.finite?</tt>
  # are true, +false+ otherwise:
  #
  #   Complex.rect(1, 1).finite?               # => true
  #   Complex.rect(Float::INFINITY, 0).finite? # => false
  #
  # Related: Numeric#finite?, Float#finite?.
  def finite?; end

  # Returns the integer hash value for +self+.
  #
  # Two \Complex objects created from the same values will have the same hash value
  # (and will compare using #eql?):
  #
  #   Complex.rect(1, 2).hash == Complex.rect(1, 2).hash # => true
  def hash; end

  # Returns the imaginary value for +self+:
  #
  #   Complex.rect(7).imag     # => 0
  #   Complex.rect(9, -4).imag # => -4
  #
  # If +self+ was created with
  # {polar coordinates}[rdoc-ref:Complex@Polar+Coordinates], the returned value
  # is computed, and may be inexact:
  #
  #   Complex.polar(1, Math::PI/4).imag # => 0.7071067811865476 # Square root of 2.
  def imaginary; end
  alias imag imaginary

  # Returns +1+ if either <tt>self.real.infinite?</tt> or <tt>self.imag.infinite?</tt>
  # is true, +nil+ otherwise:
  #
  #   Complex.rect(Float::INFINITY, 0).infinite? # => 1
  #   Complex.rect(1, 1).infinite?               # => nil
  #
  # Related: Numeric#infinite?, Float#infinite?.
  def infinite?; end

  # Returns a string representation of +self+:
  #
  #   Complex.rect(2).inspect                      # => "(2+0i)"
  #   Complex.rect(-8, 6).inspect                  # => "(-8+6i)"
  #   Complex.rect(0, Rational(1, 2)).inspect      # => "(0+(1/2)*i)"
  #   Complex.rect(0, Float::INFINITY).inspect     # => "(0+Infinity*i)"
  #   Complex.rect(Float::NAN, Float::NAN).inspect # => "(NaN+NaN*i)"
  def inspect; end

  # Returns the \Complex object created from the numerators
  # of the real and imaginary parts of +self+,
  # after converting each part to the
  # {lowest common denominator}[https://en.wikipedia.org/wiki/Lowest_common_denominator]
  # of the two:
  #
  #   c = Complex.rect(Rational(2, 3), Rational(3, 4)) # => ((2/3)+(3/4)*i)
  #   c.numerator                                      # => (8+9i)
  #
  # In this example, the lowest common denominator of the two parts is 12;
  # the two converted parts may be thought of as \Rational(8, 12) and \Rational(9, 12),
  # whose numerators, respectively, are 8 and 9;
  # so the returned value of <tt>c.numerator</tt> is <tt>Complex.rect(8, 9)</tt>.
  #
  # Related: Complex#denominator.
  def numerator; end

  # Returns the array <tt>[self.abs, self.arg]</tt>:
  #
  #   Complex.polar(1, 2).polar # => [1.0, 2.0]
  #
  # See {Polar Coordinates}[rdoc-ref:Complex@Polar+Coordinates].
  #
  # If +self+ was created with
  # {rectangular coordinates}[rdoc-ref:Complex@Rectangular+Coordinates], the returned value
  # is computed, and may be inexact:
  #
  #   Complex.rect(1, 1).polar # => [1.4142135623730951, 0.7853981633974483]
  def polar; end

  # Returns the quotient of +self+ and +numeric+:
  #
  #   Complex.rect(2, 3)  / Complex.rect(2, 3)  # => (1+0i)
  #   Complex.rect(900)   / Complex.rect(1)     # => (900+0i)
  #   Complex.rect(-2, 9) / Complex.rect(-9, 2) # => ((36/85)-(77/85)*i)
  #   Complex.rect(9, 8)  / 4                   # => ((9/4)+2i)
  #   Complex.rect(20, 9) / 9.8                 # => (2.0408163265306123+0.9183673469387754i)
  def quo(p1) end

  # Returns a Rational object whose value is exactly or approximately
  # equivalent to that of <tt>self.real</tt>.
  #
  # With no argument +epsilon+ given, returns a \Rational object
  # whose value is exactly equal to that of <tt>self.real.rationalize</tt>:
  #
  #   Complex.rect(1, 0).rationalize              # => (1/1)
  #   Complex.rect(1, Rational(0, 1)).rationalize # => (1/1)
  #   Complex.rect(3.14159, 0).rationalize        # => (314159/100000)
  #
  # With argument +epsilon+ given, returns a \Rational object
  # whose value is exactly or approximately equal to that of <tt>self.real</tt>
  # to the given precision:
  #
  #   Complex.rect(3.14159, 0).rationalize(0.1)          # => (16/5)
  #   Complex.rect(3.14159, 0).rationalize(0.01)         # => (22/7)
  #   Complex.rect(3.14159, 0).rationalize(0.001)        # => (201/64)
  #   Complex.rect(3.14159, 0).rationalize(0.0001)       # => (333/106)
  #   Complex.rect(3.14159, 0).rationalize(0.00001)      # => (355/113)
  #   Complex.rect(3.14159, 0).rationalize(0.000001)     # => (7433/2366)
  #   Complex.rect(3.14159, 0).rationalize(0.0000001)    # => (9208/2931)
  #   Complex.rect(3.14159, 0).rationalize(0.00000001)   # => (47460/15107)
  #   Complex.rect(3.14159, 0).rationalize(0.000000001)  # => (76149/24239)
  #   Complex.rect(3.14159, 0).rationalize(0.0000000001) # => (314159/100000)
  #   Complex.rect(3.14159, 0).rationalize(0.0)          # => (3537115888337719/1125899906842624)
  #
  # Related: Complex#to_r.
  def rationalize(epsilon = nil) end

  # Returns the real value for +self+:
  #
  #   Complex.rect(7).real     # => 7
  #   Complex.rect(9, -4).real # => 9
  #
  # If +self+ was created with
  # {polar coordinates}[rdoc-ref:Complex@Polar+Coordinates], the returned value
  # is computed, and may be inexact:
  #
  #   Complex.polar(1, Math::PI/4).real # => 0.7071067811865476 # Square root of 2.
  def real; end

  # Returns +false+; for compatibility with Numeric#real?.
  def real?; end

  # Returns the array <tt>[self.real, self.imag]</tt>:
  #
  #   Complex.rect(1, 2).rect # => [1, 2]
  #
  # See {Rectangular Coordinates}[rdoc-ref:Complex@Rectangular+Coordinates].
  #
  # If +self+ was created with
  # {polar coordinates}[rdoc-ref:Complex@Polar+Coordinates], the returned value
  # is computed, and may be inexact:
  #
  #   Complex.polar(1.0, 1.0).rect # => [0.5403023058681398, 0.8414709848078965]
  #
  # Complex#rectangular is an alias for Complex#rect.
  def rectangular; end
  alias rect rectangular

  # Returns +self+.
  def to_c; end

  # Returns the value of <tt>self.real</tt> as a Float, if possible:
  #
  #   Complex.rect(1, 0).to_f              # => 1.0
  #   Complex.rect(1, Rational(0, 1)).to_f # => 1.0
  #
  # Raises RangeError if <tt>self.imag</tt> is not exactly zero
  # (either <tt>Integer(0)</tt> or <tt>Rational(0, _n_)</tt>).
  def to_f; end

  # Returns the value of <tt>self.real</tt> as an Integer, if possible:
  #
  #   Complex.rect(1, 0).to_i              # => 1
  #   Complex.rect(1, Rational(0, 1)).to_i # => 1
  #
  # Raises RangeError if <tt>self.imag</tt> is not exactly zero
  # (either <tt>Integer(0)</tt> or <tt>Rational(0, _n_)</tt>).
  def to_i; end

  # Returns the value of <tt>self.real</tt> as a Rational, if possible:
  #
  #   Complex.rect(1, 0).to_r              # => (1/1)
  #   Complex.rect(1, Rational(0, 1)).to_r # => (1/1)
  #   Complex.rect(1, 0.0).to_r            # => (1/1)
  #
  # Raises RangeError if <tt>self.imag</tt> is not exactly zero
  # (either <tt>Integer(0)</tt> or <tt>Rational(0, _n_)</tt>)
  # and <tt>self.imag.to_r</tt> is not exactly zero.
  #
  # Related: Complex#rationalize.
  def to_r; end

  # Returns a string representation of +self+:
  #
  #   Complex.rect(2).to_s                      # => "2+0i"
  #   Complex.rect(-8, 6).to_s                  # => "-8+6i"
  #   Complex.rect(0, Rational(1, 2)).to_s      # => "0+1/2i"
  #   Complex.rect(0, Float::INFINITY).to_s     # => "0+Infinity*i"
  #   Complex.rect(Float::NAN, Float::NAN).to_s # => "NaN+NaN*i"
  def to_s; end
end

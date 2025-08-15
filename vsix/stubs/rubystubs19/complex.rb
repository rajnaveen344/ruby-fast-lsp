# frozen_string_literal: true

# A complex number can be represented as a paired real number with
# imaginary unit; a+bi.  Where a is real part, b is imaginary part
# and i is imaginary unit.  Real a equals complex a+0i
# mathematically.
#
# In ruby, you can create complex object with Complex, Complex::rect,
# Complex::polar or to_c method.
#
#    Complex(1)           #=> (1+0i)
#    Complex(2, 3)        #=> (2+3i)
#    Complex.polar(2, 3)  #=> (-1.9799849932008908+0.2822400161197344i)
#    3.to_c               #=> (3+0i)
#
# You can also create complex object from floating-point numbers or
# strings.
#
#    Complex(0.3)         #=> (0.3+0i)
#    Complex('0.3-0.5i')  #=> (0.3-0.5i)
#    Complex('2/3+3/4i')  #=> ((2/3)+(3/4)*i)
#    Complex('1@2')       #=> (-0.4161468365471424+0.9092974268256817i)
#
#    0.3.to_c             #=> (0.3+0i)
#    '0.3-0.5i'.to_c      #=> (0.3-0.5i)
#    '2/3+3/4i'.to_c      #=> ((2/3)+(3/4)*i)
#    '1@2'.to_c           #=> (-0.4161468365471424+0.9092974268256817i)
#
# A complex object is either an exact or an inexact number.
#
#    Complex(1, 1) / 2    #=> ((1/2)+(1/2)*i)
#    Complex(1, 1) / 2.0  #=> (0.5+0.5i)
class Complex < Numeric
  I = _

  # Returns a complex object which denotes the given polar form.
  #
  #   Complex.polar(3, 0)           #=> (3.0+0.0i)
  #   Complex.polar(3, Math::PI/2)  #=> (1.836909530733566e-16+3.0i)
  #   Complex.polar(3, Math::PI)    #=> (-3.0+3.673819061467132e-16i)
  #   Complex.polar(3, -Math::PI/2) #=> (1.836909530733566e-16-3.0i)
  def self.polar(p1, p2 = v2) end

  # Returns a complex object which denotes the given rectangular form.
  def self.rect(p1, p2 = v2) end

  # Returns a complex object which denotes the given rectangular form.
  def self.rectangular(p1, p2 = v2) end

  # Performs multiplication.
  def *(other) end

  # Performs exponentiation.
  #
  # For example:
  #
  #     Complex('i') ** 2             #=> (-1+0i)
  #     Complex(-8) ** Rational(1,3)  #=> (1.0000000000000002+1.7320508075688772i)
  def **(other) end

  # Performs addition.
  def +(other) end

  # Performs subtraction.
  def -(other) end

  # Returns negation of the value.
  def -@; end

  # Performs division.
  #
  # For example:
  #
  #     Complex(10.0) / 3  #=> (3.3333333333333335+(0/1)*i)
  #     Complex(10)   / 3  #=> ((10/3)+(0/1)*i)  # not (3+0i)
  def /(other) end

  # Returns true if cmp equals object numerically.
  def ==(other) end

  # Returns the absolute part of its polar form.
  def abs; end
  alias magnitude abs

  # Returns square of the absolute value.
  def abs2; end

  # Returns the angle part of its polar form.
  #
  #   Complex.polar(3, Math::PI/2).arg #=> 1.5707963267948966
  def arg; end
  alias angle arg
  alias phase arg

  # Returns the complex conjugate.
  def conjugate; end
  alias conj conjugate
  alias ~ conjugate

  # Returns the denominator (lcm of both denominator - real and imag).
  #
  # See numerator.
  def denominator; end

  # Performs division as each part is a float, never returns a float.
  #
  # For example:
  #
  #     Complex(11,22).fdiv(3)  #=> (3.6666666666666665+7.333333333333333i)
  def fdiv(numeric) end

  # Returns the imaginary part.
  def imaginary; end
  alias imag imaginary

  # Returns the value as a string for inspection.
  def inspect; end

  # Returns the numerator.
  #
  # For example:
  #
  #        1   2       3+4i  <-  numerator
  #        - + -i  ->  ----
  #        2   3        6    <-  denominator
  #
  #    c = Complex('1/2+2/3i')  #=> ((1/2)+(2/3)*i)
  #    n = c.numerator          #=> (3+4i)
  #    d = c.denominator        #=> 6
  #    n / d                    #=> ((1/2)+(2/3)*i)
  #    Complex(Rational(n.real, d), Rational(n.imag, d))
  #                             #=> ((1/2)+(2/3)*i)
  # See denominator.
  def numerator; end

  # Returns an array; [cmp.abs, cmp.arg].
  def polar; end

  # Performs division.
  #
  # For example:
  #
  #     Complex(10.0) / 3  #=> (3.3333333333333335+(0/1)*i)
  #     Complex(10)   / 3  #=> ((10/3)+(0/1)*i)  # not (3+0i)
  def quo(numeric) end

  # If the imaginary part is exactly 0, returns the real part as a Rational,
  # otherwise a RangeError is raised.
  def rationalize(*eps) end

  # Returns the real part.
  def real; end

  # Returns false.
  def real?; end

  # Returns an array; [cmp.real, cmp.imag].
  def rectangular; end
  alias rect rectangular

  # Returns the value as a float if possible.
  def to_f; end

  # Returns the value as an integer if possible.
  def to_i; end

  # If the imaginary part is exactly 0, returns the real part as a Rational,
  # otherwise a RangeError is raised.
  def to_r; end

  # Returns the value as a string.
  def to_s; end
end

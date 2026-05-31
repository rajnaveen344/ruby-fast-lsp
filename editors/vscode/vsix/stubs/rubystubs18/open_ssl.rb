# frozen_string_literal: true

# OSSL library init
# ---
# let rdoc know about mOSSL
# ---
# let rdoc know about mOSSL
# ---
# let rdoc know about mOSSL
# ---
# let rdoc know about mOSSL
# ---
# let rdoc know about mOSSL
# ---
# let rdoc know about mOSSL
# ---
# let rdoc know about mOSSL and mPKey
# ---
# let rdoc know about mOSSL and mPKey
# ---
# let rdoc know about mOSSL and mPKey
# ---
# let rdoc know about mOSSL
# ---
# let rdoc know about mOSSL
# ---
# let rdoc know about mOSSL
module OpenSSL
  OPENSSL_VERSION = _
  OPENSSL_VERSION_NUMBER = _
  # Constants
  VERSION = _

  def self.debug; end

  # Turns on or off CRYPTO_MEM_CHECK.
  # Also shows some debugging message on stderr.
  def self.debug=(boolean) end

  # See any remaining errors held in queue.
  #
  # Any errors you see here are probably due to a bug in ruby's OpenSSL implementation.
  def self.errors; end

  private

  def debug; end

  # Turns on or off CRYPTO_MEM_CHECK.
  # Also shows some debugging message on stderr.
  def debug=(boolean) end

  # See any remaining errors held in queue.
  #
  # Any errors you see here are probably due to a bug in ruby's OpenSSL implementation.
  def errors; end

  module ASN1
    UNIVERSAL_TAG_NAME = _

    def self.decode(p1) end

    def self.decode_all(p1) end

    def self.traverse(p1) end

    private

    def decode(p1) end

    def decode_all(p1) end

    def traverse(p1) end

    class ASN1Data
      def initialize(p1, p2, p3) end

      def to_der; end
    end

    class ASN1Error < OpenSSLError
    end

    class Constructive < ASN1Data
      include Enumerable

      def initialize(p1, p2 = v2, p3 = v3, p4 = v4) end

      def each; end

      def to_der; end
    end

    class Primitive < ASN1Data
      def initialize(p1, p2 = v2, p3 = v3, p4 = v4) end

      def to_der; end
    end
  end

  class BN
    # === Parameters
    # * +bits+ - integer
    # * +safe+ - boolean
    # * +add+ - BN
    # * +rem+ - BN
    def self.generate_prime(p1, p2 = v2, p3 = v3, p4 = v4) end

    def initialize(*several_variants) end

    def /(other) end

    def bit_set?(bit) end

    def coerce(p1) end

    def copy(p1) end

    def eql?(other) end
    alias == eql?
    alias === eql?

    # === Parameters
    # * +checks+ - integer
    def prime?(*several_variants) end

    # === Parameters
    # * +checks+ - integer
    # * +trial_div+ - boolean
    def prime_fasttest?(*several_variants) end

    def to_bn; end

    def to_i; end
    alias to_int to_i

    # === Parameters
    # * +base+ - integer
    # * * Valid values:
    # * * * 0 - MPI
    # * * * 2 - binary
    # * * * 10 - the default
    # * * * 16 - hex
    def to_s(*several_variants) end
  end

  class BNError < OpenSSLError
  end

  class Cipher
    # Returns the names of all available ciphers in an array.
    def self.ciphers; end

    # The string must contain a valid cipher name like "AES-128-CBC" or "3DES".
    #
    # A list of cipher names is available by calling OpenSSL::Cipher.ciphers.
    def initialize(string) end

    #  === Parameters
    #  +data+ is a nonempty string.
    #
    # This method is deprecated and not available in 1.9.x or later.
    def <<(data) end

    def block_size; end

    # Make sure to call .encrypt or .decrypt before using any of the following methods:
    # * [key=, iv=, random_key, random_iv, pkcs5_keyivgen]
    #
    # Internally calls EVP_CipherInit_ex(ctx, NULL, NULL, NULL, NULL, 0).
    def decrypt; end

    # Make sure to call .encrypt or .decrypt before using any of the following methods:
    # * [key=, iv=, random_key, random_iv, pkcs5_keyivgen]
    #
    # Internally calls EVP_CipherInit_ex(ctx, NULL, NULL, NULL, NULL, 1).
    def encrypt; end

    # Returns the remaining data held in the cipher object.  Further calls to update() or final() will return garbage.
    #
    # See EVP_CipherFinal_ex for further information.
    def final; end

    # Sets the cipher iv.
    #
    # Only call this method after calling cipher.encrypt or cipher.decrypt.
    def iv=(string) end

    def iv_len(gth) end

    # Sets the cipher key.
    #
    # Only call this method after calling cipher.encrypt or cipher.decrypt.
    def key=(string) end

    def key_len(gth) end

    # Sets the key length of the cipher.  If the cipher is a fixed length cipher then attempting to set the key
    # length to any value other than the fixed value is an error.
    #
    # Under normal circumstances you do not need to call this method (and probably shouldn't).
    #
    # See EVP_CIPHER_CTX_set_key_length for further information.
    def key_len=(p1) end

    # Returns the name of the cipher which may differ slightly from the original name provided.
    def name; end

    # Enables or disables padding. By default encryption operations are padded using standard block padding and the
    # padding is checked and removed when decrypting. If the pad parameter is zero then no padding is performed, the
    # total amount of data encrypted or decrypted must then be a multiple of the block size or an error will occur.
    #
    # See EVP_CIPHER_CTX_set_padding for further information.
    def padding=(integer) end

    # Generates and sets the key/iv based on a password.
    #
    # WARNING: This method is only PKCS5 v1.5 compliant when using RC2, RC4-40, or DES
    # with MD5 or SHA1.  Using anything else (like AES) will generate the key/iv using an
    # OpenSSL specific method.  Use a PKCS5 v2 key generation method instead.
    #
    # === Parameters
    # +salt+ must be an 8 byte string if provided.
    # +iterations+ is a integer with a default of 2048.
    # +digest+ is a Digest object that defaults to 'MD5'
    #
    # A minimum of 1000 iterations is recommended.
    def pkcs5_keyivgen(p1, p2 = v2, p3 = v3, p4 = v4) end

    # Internally calls EVP_CipherInit_ex(ctx, NULL, NULL, NULL, NULL, -1).
    def reset; end

    # === Parameters
    # +data+ is a nonempty string.
    # +buffer+ is an optional string to store the result.
    def update(p1, p2 = v2) end

    private

    # Returns the names of all available ciphers in an array.
    def ciphers; end

    class CipherError < OpenSSLError
    end
  end

  class Config
    DEFAULT_CONFIG_FILE = _
  end

  class ConfigError < OpenSSLError
  end

  class Digest < Class
    def initialize(string) end

    def block_length; end

    # Returns the output size of the digest.
    def digest_length; end

    def name; end

    def reset; end

    def update(string) end
    alias << update

    private

    def finish; end

    class DigestError < OpenSSLError
    end
  end

  class Engine
    def self.by_id(p1) end

    def self.cleanup; end

    def self.engines; end

    def self.load(p1 = v1) end

    def cipher(p1) end

    def cmds; end

    def ctrl_cmd(p1, p2 = v2) end

    def digest(p1) end

    def finish; end

    def id; end

    def inspect; end

    def load_private_key(p1 = v1, p2 = v2) end

    def load_public_key(p1 = v1, p2 = v2) end

    def name; end

    def set_default(p1) end

    class EngineError < OpenSSLError
    end
  end

  class HMAC
    def self.digest(digest, key, data) end

    def self.hexdigest(p1, p2, p3) end

    def initialize(key, digest) end

    def digest; end

    def hexdigest; end
    alias inspect hexdigest
    alias to_s hexdigest

    def reset; end

    def update(string) end
    alias << update
  end

  class HMACError < OpenSSLError
  end

  module Netscape
    class SPKI
      def initialize(p1 = v1) end

      def challenge; end

      def challenge=(p1) end

      def public_key; end

      def public_key=(p1) end

      def sign(p1, p2) end

      def to_der; end

      def to_pem; end
      alias to_s to_pem

      def to_text; end

      # Checks that cert signature is made with PRIVversion of this PUBLIC 'key'
      def verify(p1) end
    end

    class SPKIError < OpenSSLError
    end
  end

  module OCSP
    class BasicResponse
      def initialize(*args) end

      def add_nonce(p1 = v1) end

      def add_status(p1, p2, p3, p4, p5, p6, p7) end

      def copy_nonce(p1) end

      def sign(p1, p2, p3 = v3, p4 = v4) end

      def status; end

      def verify(p1, p2, p3 = v3) end
    end

    class CertificateId
      def initialize(p1, p2) end

      def cmp(p1) end

      def cmp_issuer(p1) end

      def serial; end
    end

    class OCSPError < OpenSSLError
    end

    class Request
      def initialize(p1 = v1) end

      def add_certid(p1) end

      def add_nonce(p1 = v1) end

      def certid; end

      # Check nonce validity in a request and response.
      # Return value reflects result:
      #  1: nonces present and equal.
      #  2: nonces both absent.
      #  3: nonce present in response only.
      #  0: nonces both present and not equal.
      # -1: nonce in request only.
      #
      #  For most responders clients can check return > 0.
      #  If responder doesn't handle nonces return != 0 may be
      #  necessary. return == 0 is always an error.
      def check_nonce(p1) end

      def sign(p1, p2, p3 = v3, p4 = v4) end

      def to_der; end

      def verify(p1, p2, p3 = v3) end
    end

    class Response
      # OCSP::Response
      def self.create(p1, p2) end

      def initialize(p1 = v1) end

      def basic; end

      def status; end

      def status_string; end

      def to_der; end
    end
  end

  # Generic error,
  # common for all classes under OpenSSL module
  class OpenSSLError < StandardError
  end

  # Defines a file format commonly used to store private keys with
  # accompanying public key certificates, protected with a password-based
  # symmetric key.
  class PKCS12
    # === Parameters
    # * +pass+ - string
    # * +name+ - A string describing the key.
    # * +key+ - Any PKey.
    # * +cert+ - A X509::Certificate.
    # * * The public_key portion of the certificate must contain a valid public key.
    # * * The not_before and not_after fields must be filled in.
    # * +ca+ - An optional array of X509::Certificate's.
    # * +key_pbe+ - string
    # * +cert_pbe+ - string
    # * +key_iter+ - integer
    # * +mac_iter+ - integer
    # * +keytype+ - An integer representing an MSIE specific extension.
    #
    # Any optional arguments may be supplied as nil to preserve the OpenSSL defaults.
    #
    # See the OpenSSL documentation for PKCS12_create().
    def self.create(p1, p2, p3, p4, p5 = v5, p6 = v6, p7 = v7, p8 = v8, p9 = v9, p10 = v10) end

    # === Parameters
    # * +str+ - Must be a DER encoded PKCS12 string.
    # * +pass+ - string
    def initialize(*several_variants) end

    def to_der; end

    class PKCS12Error < OpenSSLError
    end
  end

  # Password-based Encryption
  module PKCS5
    # === Parameters
    # * +pass+ - string
    # * +salt+ - string
    # * +iter+ - integer - should be greater than 1000.  2000 is better.
    # * +keylen+ - integer
    # * +digest+ - a string or OpenSSL::Digest object.
    #
    # Available in OpenSSL 0.9.9?.
    #
    # Digests other than SHA1 may not be supported by other cryptography libraries.
    def self.pbkdf2_hmac(pass, salt, iter, keylen, digest) end

    # === Parameters
    # * +pass+ - string
    # * +salt+ - string
    # * +iter+ - integer - should be greater than 1000.  2000 is better.
    # * +keylen+ - integer
    #
    # This method is available almost any version OpenSSL.
    #
    # Conforms to rfc2898.
    def self.pbkdf2_hmac_sha1(pass, salt, iter, keylen) end

    private

    # === Parameters
    # * +pass+ - string
    # * +salt+ - string
    # * +iter+ - integer - should be greater than 1000.  2000 is better.
    # * +keylen+ - integer
    # * +digest+ - a string or OpenSSL::Digest object.
    #
    # Available in OpenSSL 0.9.9?.
    #
    # Digests other than SHA1 may not be supported by other cryptography libraries.
    def pbkdf2_hmac(pass, salt, iter, keylen, digest) end

    # === Parameters
    # * +pass+ - string
    # * +salt+ - string
    # * +iter+ - integer - should be greater than 1000.  2000 is better.
    # * +keylen+ - integer
    #
    # This method is available almost any version OpenSSL.
    #
    # Conforms to rfc2898.
    def pbkdf2_hmac_sha1(pass, salt, iter, keylen) end

    class PKCS5Error < OpenSSLError
    end
  end

  class PKCS7
    Signer = _

    def self.encrypt(p1, p2, p3 = v3, p4 = v4) end

    def self.read_smime(string) end

    def self.sign(p1, p2, p3, p4 = v4, p5 = v5) end

    def self.write_smime(p1, p2 = v2, p3 = v3) end

    # Many methods in this class aren't documented.
    def initialize(*several_variants) end

    def add_certificate(p1) end

    def add_crl(p1) end

    def add_data(p1) end
    alias data= add_data

    def add_recipient(p1) end

    def add_signer(p1) end

    def certificates; end

    def certificates=(p1) end

    def cipher=(p1) end

    def crls; end

    def crls=(p1) end

    def decrypt(p1, p2, p3 = v3) end

    def detached; end

    def detached=(p1) end

    def detached?; end

    def recipients; end

    def signers; end

    def to_der; end

    def to_pem; end
    alias to_s to_pem

    def type; end

    def type=(type) end

    def verify(p1, p2, p3 = v3, p4 = v4) end

    class PKCS7Error < OpenSSLError
    end

    class RecipientInfo
      def initialize(p1) end

      def enc_key; end

      def issuer; end

      def serial; end
    end

    class SignerInfo
      def initialize(p1, p2, p3) end

      def issuer; end
      alias name issuer

      def serial; end

      def signed_time; end
    end
  end

  module PKey
    class DH < PKey
      # === Parameters
      # * +size+ is an integer representing the desired key size.  Keys smaller than 1024 should be considered insecure.
      # * +generator+ is a small number > 1, typically 2 or 5.
      def self.generate(p1, p2 = v2) end

      # === Parameters
      # * +size+ is an integer representing the desired key size.  Keys smaller than 1024 should be considered insecure.
      # * +generator+ is a small number > 1, typically 2 or 5.
      # * +string+ contains the DER or PEM encoded key.
      #
      # === Examples
      # * DH.new -> dh
      # * DH.new(1024) -> dh
      # * DH.new(1024, 5) -> dh
      # * DH.new(File.read('key.pem')) -> dh
      def initialize(p1 = v1, p2 = v2) end

      # === Parameters
      # * +pub_bn+ is a OpenSSL::BN.
      #
      # Returns aString containing a shared secret computed from the other parties public value.
      #
      # See DH_compute_key() for further information.
      def compute_key(pub_bn) end

      def export; end
      alias to_pem export
      alias to_s export

      def generate_key!; end

      # Stores all parameters of key to the hash
      # INSECURE: PRIVATE INFORMATIONS CAN LEAK OUT!!!
      # Don't use :-)) (I's up to you)
      def params; end

      def params_ok?; end

      def private?; end

      def public?; end

      # Makes new instance DH PUBLIC_KEY from PRIVATE_KEY
      def public_key; end

      def to_der; end

      # Prints all parameters of key to buffer
      # INSECURE: PRIVATE INFORMATIONS CAN LEAK OUT!!!
      # Don't use :-)) (I's up to you)
      def to_text; end
    end

    class DHError < PKeyError
    end

    class DSA < PKey
      # === Parameters
      # * +size+ is an integer representing the desired key size.
      def self.generate(size) end

      # === Parameters
      # * +size+ is an integer representing the desired key size.
      # * +string+ contains a DER or PEM encoded key.
      # * +pass+ is a string that contains a optional password.
      #
      # === Examples
      # * DSA.new -> dsa
      # * DSA.new(1024) -> dsa
      # * DSA.new(File.read('dsa.pem')) -> dsa
      # * DSA.new(File.read('dsa.pem'), 'mypassword') -> dsa
      def initialize(p1 = v1, p2 = v2) end

      # === Parameters
      # +cipher+ is an OpenSSL::Cipher.
      # +password+ is a string containing your password.
      #
      # === Examples
      # * DSA.to_pem -> aString
      # * DSA.to_pem(cipher, 'mypassword') -> aString
      def export(p1 = v1, p2 = v2) end
      alias to_pem export
      alias to_s export

      # Stores all parameters of key to the hash
      # INSECURE: PRIVATE INFORMATIONS CAN LEAK OUT!!!
      # Don't use :-)) (I's up to you)
      def params; end

      def private?; end

      def public?; end

      # Makes new instance DSA PUBLIC_KEY from PRIVATE_KEY
      def public_key; end

      def syssign(string) end

      def sysverify(digest, sig) end

      def to_der; end

      # Prints all parameters of key to buffer
      # INSECURE: PRIVATE INFORMATIONS CAN LEAK OUT!!!
      # Don't use :-)) (I's up to you)
      def to_text; end
    end

    class DSAError < PKeyError
    end

    class EC < PKey
      NAMED_CURVE = _

      # See the OpenSSL documentation for EC_builtin_curves()
      def self.builtin_curves; end

      # See the OpenSSL documentation for:
      #    EC_KEY_*
      def initialize(*several_variants) end

      # Raises an exception if the key is invalid.
      #
      # See the OpenSSL documentation for EC_KEY_check_key()
      def check_key; end

      # See the OpenSSL documentation for ECDH_compute_key()
      def dh_compute_key(pubkey) end

      # See the OpenSSL documentation for ECDSA_sign()
      def dsa_sign_asn1(data) end

      # See the OpenSSL documentation for ECDSA_verify()
      def dsa_verify_asn1(data, sig) end

      # See the OpenSSL documentation for EC_KEY_generate_key()
      def generate_key; end

      # Returns a constant <code>OpenSSL::EC::Group</code> that is tied to the key.
      # Modifying the returned group can make the key invalid.
      def group; end

      # Returns the same object passed, not the group object associated with the key.
      # If you wish to access the group object tied to the key call key.group after setting
      # the group.
      #
      # Setting the group will immediately destroy any previously assigned group object.
      # The group is internally copied by OpenSSL.  Modifying the original group after
      # assignment will not effect the internal key structure.
      # (your changes may be lost).  BE CAREFUL.
      #
      # EC_KEY_set_group calls EC_GROUP_free(key->group) then EC_GROUP_dup(), not EC_GROUP_copy.
      # This documentation is accurate for OpenSSL 0.9.8b.
      def group=(group) end

      # See the OpenSSL documentation for EC_KEY_get0_private_key()
      def private_key; end

      # See the OpenSSL documentation for EC_KEY_set_private_key()
      def private_key=(openssl_bn) end

      # Both public_key? and private_key? may return false at the same time unlike other PKey classes.
      def private_key?; end

      # See the OpenSSL documentation for EC_KEY_get0_public_key()
      def public_key; end

      # See the OpenSSL documentation for EC_KEY_set_public_key()
      def public_key=(ec_point) end

      # Both public_key? and private_key? may return false at the same time unlike other PKey classes.
      def public_key?; end

      # See the OpenSSL documentation for i2d_ECPrivateKey_bio()
      def to_der; end

      # See the OpenSSL documentation for PEM_write_bio_ECPrivateKey()
      def to_pem; end

      # See the OpenSSL documentation for EC_KEY_print()
      def to_text; end

      class Group
        # See the OpenSSL documentation for EC_GROUP_*
        def initialize(*several_variants) end

        # See the OpenSSL documentation for EC_GROUP_get_asn1_flag()
        def asn1_flag; end

        # See the OpenSSL documentation for EC_GROUP_set_asn1_flag()
        def asn1_flag=(p1) end

        # See the OpenSSL documentation for EC_GROUP_get_cofactor()
        def cofactor; end

        # See the OpenSSL documentation for EC_GROUP_get_curve_name()
        def curve_name; end

        # See the OpenSSL documentation for EC_GROUP_get_degree()
        def degree; end

        def eql?(other) end
        alias == eql?

        # See the OpenSSL documentation for EC_GROUP_get0_generator()
        def generator; end

        # See the OpenSSL documentation for EC_GROUP_get_order()
        def order; end

        # See the OpenSSL documentation for EC_GROUP_get_point_conversion_form()
        def point_conversion_form; end

        # See the OpenSSL documentation for EC_GROUP_set_point_conversion_form()
        def point_conversion_form=(form) end

        # See the OpenSSL documentation for EC_GROUP_get0_seed()
        def seed; end

        # See the OpenSSL documentation for EC_GROUP_set_seed()
        def seed=(seed) end

        # See the OpenSSL documentation for EC_GROUP_set_generator()
        def set_generator(generator, order, cofactor) end

        # See the OpenSSL documentation for i2d_ECPKParameters_bio()
        def to_der; end

        # See the OpenSSL documentation for PEM_write_bio_ECPKParameters()
        def to_pem; end

        # See the OpenSSL documentation for ECPKParameters_print()
        def to_text; end

        class Error < OpenSSLError
        end
      end

      class Point
        # See the OpenSSL documentation for EC_POINT_*
        def initialize(*several_variants) end

        def eql?(other) end
        alias == eql?

        def infinity?; end

        def invert!; end

        def make_affine!; end

        def on_curve?; end

        def set_to_infinity!; end

        # See the OpenSSL documentation for EC_POINT_point2bn()
        def to_bn; end

        class Error < OpenSSLError
        end
      end
    end

    class ECError < PKeyError
    end

    class PKey
      def initialize; end

      def sign(p1, p2) end

      def verify(p1, p2, p3) end
    end

    class PKeyError < OpenSSLError
    end

    class RSA < PKey
      # === Parameters
      # * +size+ is an integer representing the desired key size.  Keys smaller than 1024 should be considered insecure.
      # * +exponent+ is an odd number normally 3, 17, or 65537.
      def self.generate(p1, p2 = v2) end

      # === Parameters
      # * +size+ is an integer representing the desired key size.
      # * +encoded_key+ is a string containing PEM or DER encoded key.
      # * +pass+ is an optional string with the password to decrypt the encoded key.
      #
      # === Examples
      # * RSA.new(2048) -> rsa
      # * RSA.new(File.read("rsa.pem")) -> rsa
      # * RSA.new(File.read("rsa.pem"), "mypassword") -> rsa
      def initialize(p1 = v1, p2 = v2) end

      def blinding_off!; end

      def blinding_on!; end

      # === Parameters
      # * +cipher+ is a Cipher object.
      # * +pass+ is a string.
      #
      # === Examples
      # * rsa.to_pem -> aString
      # * rsa.to_pem(cipher, pass) -> aString
      def export(p1 = v1, p2 = v2) end
      alias to_pem export
      alias to_s export

      # Stores all parameters of key to the hash
      # INSECURE: PRIVATE INFORMATIONS CAN LEAK OUT!!!
      # Don't use :-)) (I's up to you)
      def params; end

      def private?; end

      def private_decrypt(p1, p2 = v2) end

      def private_encrypt(p1, p2 = v2) end

      # The return value is always true since every private key is also a public key.
      def public?; end

      def public_decrypt(p1, p2 = v2) end

      def public_encrypt(p1, p2 = v2) end

      # Makes new instance RSA PUBLIC_KEY from PRIVATE_KEY
      def public_key; end

      def to_der; end

      # Prints all parameters of key to buffer
      # INSECURE: PRIVATE INFORMATIONS CAN LEAK OUT!!!
      # Don't use :-)) (It's up to you)
      def to_text; end
    end

    class RSAError < PKeyError
    end
  end

  module Random
    class RandomError < OpenSSLError
    end
  end

  module SSL
    # class SSLContext
    #
    #     The following attributes are available but don't show up in rdoc.
    #     All attributes must be set before calling SSLSocket.new(io, ctx).
    #     * ssl_version, cert, key, client_ca, ca_file, ca_path, timeout,
    #     * verify_mode, verify_depth client_cert_cb, tmp_dh_callback,
    #     * session_id_context, session_add_cb, session_new_cb, session_remove_cb
    class SSLContext
      # holds a list of available SSL/TLS methods
      METHODS = _
      SESSION_CACHE_BOTH = _
      SESSION_CACHE_CLIENT = _
      SESSION_CACHE_NO_AUTO_CLEAR = _
      SESSION_CACHE_NO_INTERNAL = _
      SESSION_CACHE_NO_INTERNAL_LOOKUP = _
      SESSION_CACHE_NO_INTERNAL_STORE = _
      SESSION_CACHE_OFF = _
      SESSION_CACHE_SERVER = _

      # You can get a list of valid methods with OpenSSL::SSL::SSLContext::METHODS
      def initialize(*several_variants) end

      def ciphers; end

      def ciphers=(p1) end

      def flush_sessions(p1 = v1) end

      def session_add(session) end

      def session_cache_mode; end

      def session_cache_mode=(integer) end

      def session_cache_size; end

      def session_cache_size=(integer) end

      def session_cache_stats; end

      def session_remove(session) end

      # This method is called automatically when a new SSLSocket is created.
      # Normally you do not need to call this method (unless you are writing an extension in C).
      def setup; end

      def ssl_version=(p1) end
    end

    class SSLError < OpenSSLError
    end

    # class SSLSocket
    #
    #     The following attributes are available but don't show up in rdoc.
    #     * io, context, sync_close
    class SSLSocket
      # === Parameters
      # * +io+ is a real ruby IO object.  Not an IO like object that responds to read/write.
      # * +ctx+ is an OpenSSLSSL::SSLContext.
      #
      # The OpenSSL::Buffering module provides additional IO methods.
      #
      # This method will freeze the SSLContext if one is provided;
      # however, session management is still allowed in the frozen SSLContext.
      def initialize(*several_variants) end

      def accept; end

      def cert; end

      def cipher; end

      def connect; end

      def peer_cert; end

      def peer_cert_chain; end

      def pending; end

      def session=(session) end

      def session_reused?; end

      def state; end

      def sysclose; end

      # === Parameters
      # * +length+ is a positive integer.
      # * +buffer+ is a string used to store the result.
      def sysread(*several_variants) end

      def syswrite(string) end

      def verify_result; end
    end

    class Session
      # === Parameters
      # +SSLSocket+ is an OpenSSL::SSL::SSLSocket
      # +string+ must be a DER or PEM encoded Session.
      def initialize(p1) end

      def ==(other) end

      # Returns the Session ID.
      def id; end

      def time; end

      # How long until the session expires in seconds.
      def timeout; end

      # Returns an ASN1 encoded String that contains the Session object.
      def to_der; end

      # Returns a PEM encoded String that contains the Session object.
      def to_pem; end

      # Shows everything in the Session object.
      def to_text; end

      class SessionError < OpenSSLError
      end
    end
  end

  module X509
    class Attribute
      def initialize(p1, p2 = v2) end

      def oid; end

      def oid=(string) end

      def to_der; end

      def value; end

      def value=(asn1) end
    end

    class AttributeError < OpenSSLError
    end

    class CRL
      def initialize(p1 = v1) end

      def add_extension(p1) end

      def add_revoked(p1) end

      # Gets X509v3 extensions as array of X509Ext objects
      def extensions; end

      # Sets X509_EXTENSIONs
      def extensions=(p1) end

      def issuer; end

      def issuer=(p1) end

      def last_update; end

      def last_update=(p1) end

      def next_update; end

      def next_update=(p1) end

      def revoked; end

      def revoked=(p1) end

      def sign(p1, p2) end

      def signature_algorithm; end

      def to_der; end

      def to_pem; end
      alias to_s to_pem

      def to_text; end

      def verify(p1) end

      def version; end

      def version=(p1) end
    end

    class CRLError < OpenSSLError
    end

    class Certificate
      def initialize(*several_variants) end

      def add_extension(extension) end

      # Checks if 'key' is PRIV key for this cert
      def check_private_key(key) end

      def extensions; end

      def extensions=(p1) end

      def inspect; end

      def issuer; end

      def issuer=(name) end

      def not_after; end

      def not_after=(p1) end

      def not_before; end

      def not_before=(time) end

      def public_key; end

      def public_key=(key) end

      def serial; end

      def serial=(integer) end

      def sign(key, digest) end

      def signature_algorithm; end

      def subject; end

      def subject=(name) end

      def to_der; end

      def to_pem; end
      alias to_s to_pem

      def to_text; end

      # Checks that cert signature is made with PRIVversion of this PUBLIC 'key'
      def verify(key) end

      def version; end

      def version=(integer) end
    end

    class CertificateError < OpenSSLError
    end

    class Extension
      def initialize(p1, p2 = v2, p3 = v3) end

      def critical=(p1) end

      def critical?; end

      def oid; end

      def oid=(p1) end

      def to_der; end

      def value; end

      def value=(p1) end
    end

    class ExtensionError < OpenSSLError
    end

    class ExtensionFactory
      def initialize(p1 = v1, p2 = v2, p3 = v3, p4 = v4) end

      def config=(p1) end

      # Array to X509_EXTENSION
      # Structure:
      # ["ln", "value", bool_critical] or
      # ["sn", "value", bool_critical] or
      # ["ln", "critical,value"] or the same for sn
      # ["ln", "value"] => not critical
      def create_ext(p1, p2, p3 = v3) end

      def crl=(p1) end

      def issuer_certificate=(p1) end

      def subject_certificate=(p1) end

      def subject_request=(p1) end
    end

    class Name
      COMPAT = _
      DEFAULT_OBJECT_TYPE = _
      MULTILINE = _
      OBJECT_TYPE_TEMPLATE = _
      ONELINE = _
      RFC2253 = _

      def initialize(*several_variants) end

      def add_entry(p1, p2, p3 = v3) end

      def cmp(p1) end
      alias <=> cmp

      def eql?(other) end

      def hash; end

      # hash_old returns MD5 based hash used in OpenSSL 0.9.X.
      def hash_old; end

      def to_a; end

      def to_der; end

      def to_s(*several_variants) end
    end

    class NameError < OpenSSLError
    end

    class Request
      def initialize(p1 = v1) end

      def add_attribute(p1) end

      def attributes; end

      def attributes=(p1) end

      def public_key; end

      def public_key=(p1) end

      def sign(p1, p2) end

      def signature_algorithm; end

      def subject; end

      def subject=(p1) end

      def to_der; end

      def to_pem; end
      alias to_s to_pem

      def to_text; end

      # Checks that cert signature is made with PRIVversion of this PUBLIC 'key'
      def verify(p1) end

      def version; end

      def version=(p1) end
    end

    class RequestError < OpenSSLError
    end

    class Revoked
      def initialize(*args) end

      def add_extension(p1) end

      # Gets X509v3 extensions as array of X509Ext objects
      def extensions; end

      # Sets X509_EXTENSIONs
      def extensions=(p1) end

      def serial; end

      def serial=(p1) end

      def time; end

      def time=(p1) end
    end

    class RevokedError < OpenSSLError
    end

    class Store
      def initialize; end

      def add_cert(p1) end

      def add_crl(p1) end

      def add_file(p1) end

      def add_path(p1) end

      def flags=(p1) end

      def purpose=(p1) end

      def set_default_paths; end

      def time=(p1) end

      def trust=(p1) end

      def verify(p1, p2 = v2) end

      # General callback for OpenSSL verify
      def verify_callback=(p1) end
    end

    class StoreContext
    end

    class StoreError < OpenSSLError
    end
  end
end

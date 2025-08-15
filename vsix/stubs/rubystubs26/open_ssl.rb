# frozen_string_literal: true

# OpenSSL provides SSL, TLS and general purpose cryptography.  It wraps the
# OpenSSL[https://www.openssl.org/] library.
#
# = Examples
#
# All examples assume you have loaded OpenSSL with:
#
#   require 'openssl'
#
# These examples build atop each other.  For example the key created in the
# next is used in throughout these examples.
#
# == Keys
#
# === Creating a Key
#
# This example creates a 2048 bit RSA keypair and writes it to the current
# directory.
#
#   key = OpenSSL::PKey::RSA.new 2048
#
#   open 'private_key.pem', 'w' do |io| io.write key.to_pem end
#   open 'public_key.pem', 'w' do |io| io.write key.public_key.to_pem end
#
# === Exporting a Key
#
# Keys saved to disk without encryption are not secure as anyone who gets
# ahold of the key may use it unless it is encrypted.  In order to securely
# export a key you may export it with a pass phrase.
#
#   cipher = OpenSSL::Cipher.new 'AES-128-CBC'
#   pass_phrase = 'my secure pass phrase goes here'
#
#   key_secure = key.export cipher, pass_phrase
#
#   open 'private.secure.pem', 'w' do |io|
#     io.write key_secure
#   end
#
# OpenSSL::Cipher.ciphers returns a list of available ciphers.
#
# === Loading a Key
#
# A key can also be loaded from a file.
#
#   key2 = OpenSSL::PKey::RSA.new File.read 'private_key.pem'
#   key2.public? # => true
#   key2.private? # => true
#
# or
#
#   key3 = OpenSSL::PKey::RSA.new File.read 'public_key.pem'
#   key3.public? # => true
#   key3.private? # => false
#
# === Loading an Encrypted Key
#
# OpenSSL will prompt you for your pass phrase when loading an encrypted key.
# If you will not be able to type in the pass phrase you may provide it when
# loading the key:
#
#   key4_pem = File.read 'private.secure.pem'
#   pass_phrase = 'my secure pass phrase goes here'
#   key4 = OpenSSL::PKey::RSA.new key4_pem, pass_phrase
#
# == RSA Encryption
#
# RSA provides encryption and decryption using the public and private keys.
# You can use a variety of padding methods depending upon the intended use of
# encrypted data.
#
# === Encryption & Decryption
#
# Asymmetric public/private key encryption is slow and victim to attack in
# cases where it is used without padding or directly to encrypt larger chunks
# of data. Typical use cases for RSA encryption involve "wrapping" a symmetric
# key with the public key of the recipient who would "unwrap" that symmetric
# key again using their private key.
# The following illustrates a simplified example of such a key transport
# scheme. It shouldn't be used in practice, though, standardized protocols
# should always be preferred.
#
#   wrapped_key = key.public_encrypt key
#
# A symmetric key encrypted with the public key can only be decrypted with
# the corresponding private key of the recipient.
#
#   original_key = key.private_decrypt wrapped_key
#
# By default PKCS#1 padding will be used, but it is also possible to use
# other forms of padding, see PKey::RSA for further details.
#
# === Signatures
#
# Using "private_encrypt" to encrypt some data with the private key is
# equivalent to applying a digital signature to the data. A verifying
# party may validate the signature by comparing the result of decrypting
# the signature with "public_decrypt" to the original data. However,
# OpenSSL::PKey already has methods "sign" and "verify" that handle
# digital signatures in a standardized way - "private_encrypt" and
# "public_decrypt" shouldn't be used in practice.
#
# To sign a document, a cryptographically secure hash of the document is
# computed first, which is then signed using the private key.
#
#   digest = OpenSSL::Digest::SHA256.new
#   signature = key.sign digest, document
#
# To validate the signature, again a hash of the document is computed and
# the signature is decrypted using the public key. The result is then
# compared to the hash just computed, if they are equal the signature was
# valid.
#
#   digest = OpenSSL::Digest::SHA256.new
#   if key.verify digest, signature, document
#     puts 'Valid'
#   else
#     puts 'Invalid'
#   end
#
# == PBKDF2 Password-based Encryption
#
# If supported by the underlying OpenSSL version used, Password-based
# Encryption should use the features of PKCS5. If not supported or if
# required by legacy applications, the older, less secure methods specified
# in RFC 2898 are also supported (see below).
#
# PKCS5 supports PBKDF2 as it was specified in PKCS#5
# v2.0[http://www.rsa.com/rsalabs/node.asp?id=2127]. It still uses a
# password, a salt, and additionally a number of iterations that will
# slow the key derivation process down. The slower this is, the more work
# it requires being able to brute-force the resulting key.
#
# === Encryption
#
# The strategy is to first instantiate a Cipher for encryption, and
# then to generate a random IV plus a key derived from the password
# using PBKDF2. PKCS #5 v2.0 recommends at least 8 bytes for the salt,
# the number of iterations largely depends on the hardware being used.
#
#   cipher = OpenSSL::Cipher.new 'AES-128-CBC'
#   cipher.encrypt
#   iv = cipher.random_iv
#
#   pwd = 'some hopefully not to easily guessable password'
#   salt = OpenSSL::Random.random_bytes 16
#   iter = 20000
#   key_len = cipher.key_len
#   digest = OpenSSL::Digest::SHA256.new
#
#   key = OpenSSL::PKCS5.pbkdf2_hmac(pwd, salt, iter, key_len, digest)
#   cipher.key = key
#
#   Now encrypt the data:
#
#   encrypted = cipher.update document
#   encrypted << cipher.final
#
# === Decryption
#
# Use the same steps as before to derive the symmetric AES key, this time
# setting the Cipher up for decryption.
#
#   cipher = OpenSSL::Cipher.new 'AES-128-CBC'
#   cipher.decrypt
#   cipher.iv = iv # the one generated with #random_iv
#
#   pwd = 'some hopefully not to easily guessable password'
#   salt = ... # the one generated above
#   iter = 20000
#   key_len = cipher.key_len
#   digest = OpenSSL::Digest::SHA256.new
#
#   key = OpenSSL::PKCS5.pbkdf2_hmac(pwd, salt, iter, key_len, digest)
#   cipher.key = key
#
#   Now decrypt the data:
#
#   decrypted = cipher.update encrypted
#   decrypted << cipher.final
#
# == PKCS #5 Password-based Encryption
#
# PKCS #5 is a password-based encryption standard documented at
# RFC2898[http://www.ietf.org/rfc/rfc2898.txt].  It allows a short password or
# passphrase to be used to create a secure encryption key. If possible, PBKDF2
# as described above should be used if the circumstances allow it.
#
# PKCS #5 uses a Cipher, a pass phrase and a salt to generate an encryption
# key.
#
#   pass_phrase = 'my secure pass phrase goes here'
#   salt = '8 octets'
#
# === Encryption
#
# First set up the cipher for encryption
#
#   encryptor = OpenSSL::Cipher.new 'AES-128-CBC'
#   encryptor.encrypt
#   encryptor.pkcs5_keyivgen pass_phrase, salt
#
# Then pass the data you want to encrypt through
#
#   encrypted = encryptor.update 'top secret document'
#   encrypted << encryptor.final
#
# === Decryption
#
# Use a new Cipher instance set up for decryption
#
#   decryptor = OpenSSL::Cipher.new 'AES-128-CBC'
#   decryptor.decrypt
#   decryptor.pkcs5_keyivgen pass_phrase, salt
#
# Then pass the data you want to decrypt through
#
#   plain = decryptor.update encrypted
#   plain << decryptor.final
#
# == X509 Certificates
#
# === Creating a Certificate
#
# This example creates a self-signed certificate using an RSA key and a SHA1
# signature.
#
#   key = OpenSSL::PKey::RSA.new 2048
#   name = OpenSSL::X509::Name.parse 'CN=nobody/DC=example'
#
#   cert = OpenSSL::X509::Certificate.new
#   cert.version = 2
#   cert.serial = 0
#   cert.not_before = Time.now
#   cert.not_after = Time.now + 3600
#
#   cert.public_key = key.public_key
#   cert.subject = name
#
# === Certificate Extensions
#
# You can add extensions to the certificate with
# OpenSSL::SSL::ExtensionFactory to indicate the purpose of the certificate.
#
#   extension_factory = OpenSSL::X509::ExtensionFactory.new nil, cert
#
#   cert.add_extension \
#     extension_factory.create_extension('basicConstraints', 'CA:FALSE', true)
#
#   cert.add_extension \
#     extension_factory.create_extension(
#       'keyUsage', 'keyEncipherment,dataEncipherment,digitalSignature')
#
#   cert.add_extension \
#     extension_factory.create_extension('subjectKeyIdentifier', 'hash')
#
# The list of supported extensions (and in some cases their possible values)
# can be derived from the "objects.h" file in the OpenSSL source code.
#
# === Signing a Certificate
#
# To sign a certificate set the issuer and use OpenSSL::X509::Certificate#sign
# with a digest algorithm.  This creates a self-signed cert because we're using
# the same name and key to sign the certificate as was used to create the
# certificate.
#
#   cert.issuer = name
#   cert.sign key, OpenSSL::Digest::SHA1.new
#
#   open 'certificate.pem', 'w' do |io| io.write cert.to_pem end
#
# === Loading a Certificate
#
# Like a key, a cert can also be loaded from a file.
#
#   cert2 = OpenSSL::X509::Certificate.new File.read 'certificate.pem'
#
# === Verifying a Certificate
#
# Certificate#verify will return true when a certificate was signed with the
# given public key.
#
#   raise 'certificate can not be verified' unless cert2.verify key
#
# == Certificate Authority
#
# A certificate authority (CA) is a trusted third party that allows you to
# verify the ownership of unknown certificates.  The CA issues key signatures
# that indicate it trusts the user of that key.  A user encountering the key
# can verify the signature by using the CA's public key.
#
# === CA Key
#
# CA keys are valuable, so we encrypt and save it to disk and make sure it is
# not readable by other users.
#
#   ca_key = OpenSSL::PKey::RSA.new 2048
#   pass_phrase = 'my secure pass phrase goes here'
#
#   cipher = OpenSSL::Cipher.new 'AES-128-CBC'
#
#   open 'ca_key.pem', 'w', 0400 do |io|
#     io.write ca_key.export(cipher, pass_phrase)
#   end
#
# === CA Certificate
#
# A CA certificate is created the same way we created a certificate above, but
# with different extensions.
#
#   ca_name = OpenSSL::X509::Name.parse 'CN=ca/DC=example'
#
#   ca_cert = OpenSSL::X509::Certificate.new
#   ca_cert.serial = 0
#   ca_cert.version = 2
#   ca_cert.not_before = Time.now
#   ca_cert.not_after = Time.now + 86400
#
#   ca_cert.public_key = ca_key.public_key
#   ca_cert.subject = ca_name
#   ca_cert.issuer = ca_name
#
#   extension_factory = OpenSSL::X509::ExtensionFactory.new
#   extension_factory.subject_certificate = ca_cert
#   extension_factory.issuer_certificate = ca_cert
#
#   ca_cert.add_extension \
#     extension_factory.create_extension('subjectKeyIdentifier', 'hash')
#
# This extension indicates the CA's key may be used as a CA.
#
#   ca_cert.add_extension \
#     extension_factory.create_extension('basicConstraints', 'CA:TRUE', true)
#
# This extension indicates the CA's key may be used to verify signatures on
# both certificates and certificate revocations.
#
#   ca_cert.add_extension \
#     extension_factory.create_extension(
#       'keyUsage', 'cRLSign,keyCertSign', true)
#
# Root CA certificates are self-signed.
#
#   ca_cert.sign ca_key, OpenSSL::Digest::SHA1.new
#
# The CA certificate is saved to disk so it may be distributed to all the
# users of the keys this CA will sign.
#
#   open 'ca_cert.pem', 'w' do |io|
#     io.write ca_cert.to_pem
#   end
#
# === Certificate Signing Request
#
# The CA signs keys through a Certificate Signing Request (CSR).  The CSR
# contains the information necessary to identify the key.
#
#   csr = OpenSSL::X509::Request.new
#   csr.version = 0
#   csr.subject = name
#   csr.public_key = key.public_key
#   csr.sign key, OpenSSL::Digest::SHA1.new
#
# A CSR is saved to disk and sent to the CA for signing.
#
#   open 'csr.pem', 'w' do |io|
#     io.write csr.to_pem
#   end
#
# === Creating a Certificate from a CSR
#
# Upon receiving a CSR the CA will verify it before signing it.  A minimal
# verification would be to check the CSR's signature.
#
#   csr = OpenSSL::X509::Request.new File.read 'csr.pem'
#
#   raise 'CSR can not be verified' unless csr.verify csr.public_key
#
# After verification a certificate is created, marked for various usages,
# signed with the CA key and returned to the requester.
#
#   csr_cert = OpenSSL::X509::Certificate.new
#   csr_cert.serial = 0
#   csr_cert.version = 2
#   csr_cert.not_before = Time.now
#   csr_cert.not_after = Time.now + 600
#
#   csr_cert.subject = csr.subject
#   csr_cert.public_key = csr.public_key
#   csr_cert.issuer = ca_cert.subject
#
#   extension_factory = OpenSSL::X509::ExtensionFactory.new
#   extension_factory.subject_certificate = csr_cert
#   extension_factory.issuer_certificate = ca_cert
#
#   csr_cert.add_extension \
#     extension_factory.create_extension('basicConstraints', 'CA:FALSE')
#
#   csr_cert.add_extension \
#     extension_factory.create_extension(
#       'keyUsage', 'keyEncipherment,dataEncipherment,digitalSignature')
#
#   csr_cert.add_extension \
#     extension_factory.create_extension('subjectKeyIdentifier', 'hash')
#
#   csr_cert.sign ca_key, OpenSSL::Digest::SHA1.new
#
#   open 'csr_cert.pem', 'w' do |io|
#     io.write csr_cert.to_pem
#   end
#
# == SSL and TLS Connections
#
# Using our created key and certificate we can create an SSL or TLS connection.
# An SSLContext is used to set up an SSL session.
#
#   context = OpenSSL::SSL::SSLContext.new
#
# === SSL Server
#
# An SSL server requires the certificate and private key to communicate
# securely with its clients:
#
#   context.cert = cert
#   context.key = key
#
# Then create an SSLServer with a TCP server socket and the context.  Use the
# SSLServer like an ordinary TCP server.
#
#   require 'socket'
#
#   tcp_server = TCPServer.new 5000
#   ssl_server = OpenSSL::SSL::SSLServer.new tcp_server, context
#
#   loop do
#     ssl_connection = ssl_server.accept
#
#     data = connection.gets
#
#     response = "I got #{data.dump}"
#     puts response
#
#     connection.puts "I got #{data.dump}"
#     connection.close
#   end
#
# === SSL client
#
# An SSL client is created with a TCP socket and the context.
# SSLSocket#connect must be called to initiate the SSL handshake and start
# encryption.  A key and certificate are not required for the client socket.
#
# Note that SSLSocket#close doesn't close the underlying socket by default. Set
# SSLSocket#sync_close to true if you want.
#
#   require 'socket'
#
#   tcp_socket = TCPSocket.new 'localhost', 5000
#   ssl_client = OpenSSL::SSL::SSLSocket.new tcp_socket, context
#   ssl_client.sync_close = true
#   ssl_client.connect
#
#   ssl_client.puts "hello server!"
#   puts ssl_client.gets
#
#   ssl_client.close # shutdown the TLS connection and close tcp_socket
#
# === Peer Verification
#
# An unverified SSL connection does not provide much security.  For enhanced
# security the client or server can verify the certificate of its peer.
#
# The client can be modified to verify the server's certificate against the
# certificate authority's certificate:
#
#   context.ca_file = 'ca_cert.pem'
#   context.verify_mode = OpenSSL::SSL::VERIFY_PEER
#
#   require 'socket'
#
#   tcp_socket = TCPSocket.new 'localhost', 5000
#   ssl_client = OpenSSL::SSL::SSLSocket.new tcp_socket, context
#   ssl_client.connect
#
#   ssl_client.puts "hello server!"
#   puts ssl_client.gets
#
# If the server certificate is invalid or <tt>context.ca_file</tt> is not set
# when verifying peers an OpenSSL::SSL::SSLError will be raised.
module OpenSSL
  # Boolean indicating whether OpenSSL is FIPS-capable or not
  OPENSSL_FIPS = _
  OPENSSL_LIBRARY_VERSION = _
  # Version of OpenSSL the ruby OpenSSL extension was built with
  OPENSSL_VERSION = _
  # Version number of OpenSSL the ruby OpenSSL extension was built with
  # (base 16)
  OPENSSL_VERSION_NUMBER = _
  # OpenSSL ruby extension version
  VERSION = _

  def self.debug; end

  # Turns on or off debug mode. With debug mode, all erros added to the OpenSSL
  # error queue will be printed to stderr.
  def self.debug=(boolean) end

  # See any remaining errors held in queue.
  #
  # Any errors you see here are probably due to a bug in Ruby's OpenSSL
  # implementation.
  def self.errors; end

  def self.fips_mode; end

  # Turns FIPS mode on or off. Turning on FIPS mode will obviously only have an
  # effect for FIPS-capable installations of the OpenSSL library. Trying to do
  # so otherwise will result in an error.
  #
  # === Examples
  #   OpenSSL.fips_mode = true   # turn FIPS mode on
  #   OpenSSL.fips_mode = false  # and off again
  def self.fips_mode=(boolean) end

  # Calls CRYPTO_mem_ctrl(CRYPTO_MEM_CHECK_ON). Starts tracking memory
  # allocations. See also OpenSSL.print_mem_leaks.
  #
  # This is available only when built with a capable OpenSSL and --enable-debug
  # configure option.
  def self.mem_check_start; end

  # For debugging the Ruby/OpenSSL library. Calls CRYPTO_mem_leaks_fp(stderr).
  # Prints detected memory leaks to standard error. This cleans the global state
  # up thus you cannot use any methods of the library after calling this.
  #
  # Returns +true+ if leaks detected, +false+ otherwise.
  #
  # This is available only when built with a capable OpenSSL and --enable-debug
  # configure option.
  #
  # === Example
  #   OpenSSL.mem_check_start
  #   NOT_GCED = OpenSSL::PKey::RSA.new(256)
  #
  #   END {
  #     GC.start
  #     OpenSSL.print_mem_leaks # will print the leakage
  #   }
  def self.print_mem_leaks; end

  private

  def debug; end

  # Turns on or off debug mode. With debug mode, all erros added to the OpenSSL
  # error queue will be printed to stderr.
  def debug=(boolean) end

  # See any remaining errors held in queue.
  #
  # Any errors you see here are probably due to a bug in Ruby's OpenSSL
  # implementation.
  def errors; end

  def fips_mode; end

  # Turns FIPS mode on or off. Turning on FIPS mode will obviously only have an
  # effect for FIPS-capable installations of the OpenSSL library. Trying to do
  # so otherwise will result in an error.
  #
  # === Examples
  #   OpenSSL.fips_mode = true   # turn FIPS mode on
  #   OpenSSL.fips_mode = false  # and off again
  def fips_mode=(boolean) end

  # Calls CRYPTO_mem_ctrl(CRYPTO_MEM_CHECK_ON). Starts tracking memory
  # allocations. See also OpenSSL.print_mem_leaks.
  #
  # This is available only when built with a capable OpenSSL and --enable-debug
  # configure option.
  def mem_check_start; end

  # For debugging the Ruby/OpenSSL library. Calls CRYPTO_mem_leaks_fp(stderr).
  # Prints detected memory leaks to standard error. This cleans the global state
  # up thus you cannot use any methods of the library after calling this.
  #
  # Returns +true+ if leaks detected, +false+ otherwise.
  #
  # This is available only when built with a capable OpenSSL and --enable-debug
  # configure option.
  #
  # === Example
  #   OpenSSL.mem_check_start
  #   NOT_GCED = OpenSSL::PKey::RSA.new(256)
  #
  #   END {
  #     GC.start
  #     OpenSSL.print_mem_leaks # will print the leakage
  #   }
  def print_mem_leaks; end

  # Abstract Syntax Notation One (or ASN.1) is a notation syntax to
  # describe data structures and is defined in ITU-T X.680. ASN.1 itself
  # does not mandate any encoding or parsing rules, but usually ASN.1 data
  # structures are encoded using the Distinguished Encoding Rules (DER) or
  # less often the Basic Encoding Rules (BER) described in ITU-T X.690. DER
  # and BER encodings are binary Tag-Length-Value (TLV) encodings that are
  # quite concise compared to other popular data description formats such
  # as XML, JSON etc.
  # ASN.1 data structures are very common in cryptographic applications,
  # e.g. X.509 public key certificates or certificate revocation lists
  # (CRLs) are all defined in ASN.1 and DER-encoded. ASN.1, DER and BER are
  # the building blocks of applied cryptography.
  # The ASN1 module provides the necessary classes that allow generation
  # of ASN.1 data structures and the methods to encode them using a DER
  # encoding. The decode method allows parsing arbitrary BER-/DER-encoded
  # data to a Ruby object that can then be modified and re-encoded at will.
  #
  # == ASN.1 class hierarchy
  #
  # The base class representing ASN.1 structures is ASN1Data. ASN1Data offers
  # attributes to read and set the _tag_, the _tag_class_ and finally the
  # _value_ of a particular ASN.1 item. Upon parsing, any tagged values
  # (implicit or explicit) will be represented by ASN1Data instances because
  # their "real type" can only be determined using out-of-band information
  # from the ASN.1 type declaration. Since this information is normally
  # known when encoding a type, all sub-classes of ASN1Data offer an
  # additional attribute _tagging_ that allows to encode a value implicitly
  # (+:IMPLICIT+) or explicitly (+:EXPLICIT+).
  #
  # === Constructive
  #
  # Constructive is, as its name implies, the base class for all
  # constructed encodings, i.e. those that consist of several values,
  # opposed to "primitive" encodings with just one single value. The value of
  # an Constructive is always an Array.
  #
  # ==== ASN1::Set and ASN1::Sequence
  #
  # The most common constructive encodings are SETs and SEQUENCEs, which is
  # why there are two sub-classes of Constructive representing each of
  # them.
  #
  # === Primitive
  #
  # This is the super class of all primitive values. Primitive
  # itself is not used when parsing ASN.1 data, all values are either
  # instances of a corresponding sub-class of Primitive or they are
  # instances of ASN1Data if the value was tagged implicitly or explicitly.
  # Please cf. Primitive documentation for details on sub-classes and
  # their respective mappings of ASN.1 data types to Ruby objects.
  #
  # == Possible values for _tagging_
  #
  # When constructing an ASN1Data object the ASN.1 type definition may
  # require certain elements to be either implicitly or explicitly tagged.
  # This can be achieved by setting the _tagging_ attribute manually for
  # sub-classes of ASN1Data. Use the symbol +:IMPLICIT+ for implicit
  # tagging and +:EXPLICIT+ if the element requires explicit tagging.
  #
  # == Possible values for _tag_class_
  #
  # It is possible to create arbitrary ASN1Data objects that also support
  # a PRIVATE or APPLICATION tag class. Possible values for the _tag_class_
  # attribute are:
  # * +:UNIVERSAL+ (the default for untagged values)
  # * +:CONTEXT_SPECIFIC+ (the default for tagged values)
  # * +:APPLICATION+
  # * +:PRIVATE+
  #
  # == Tag constants
  #
  # There is a constant defined for each universal tag:
  # * OpenSSL::ASN1::EOC (0)
  # * OpenSSL::ASN1::BOOLEAN (1)
  # * OpenSSL::ASN1::INTEGER (2)
  # * OpenSSL::ASN1::BIT_STRING (3)
  # * OpenSSL::ASN1::OCTET_STRING (4)
  # * OpenSSL::ASN1::NULL (5)
  # * OpenSSL::ASN1::OBJECT (6)
  # * OpenSSL::ASN1::ENUMERATED (10)
  # * OpenSSL::ASN1::UTF8STRING (12)
  # * OpenSSL::ASN1::SEQUENCE (16)
  # * OpenSSL::ASN1::SET (17)
  # * OpenSSL::ASN1::NUMERICSTRING (18)
  # * OpenSSL::ASN1::PRINTABLESTRING (19)
  # * OpenSSL::ASN1::T61STRING (20)
  # * OpenSSL::ASN1::VIDEOTEXSTRING (21)
  # * OpenSSL::ASN1::IA5STRING (22)
  # * OpenSSL::ASN1::UTCTIME (23)
  # * OpenSSL::ASN1::GENERALIZEDTIME (24)
  # * OpenSSL::ASN1::GRAPHICSTRING (25)
  # * OpenSSL::ASN1::ISO64STRING (26)
  # * OpenSSL::ASN1::GENERALSTRING (27)
  # * OpenSSL::ASN1::UNIVERSALSTRING (28)
  # * OpenSSL::ASN1::BMPSTRING (30)
  #
  # == UNIVERSAL_TAG_NAME constant
  #
  # An Array that stores the name of a given tag number. These names are
  # the same as the name of the tag constant that is additionally defined,
  # e.g. UNIVERSAL_TAG_NAME[2] = "INTEGER" and OpenSSL::ASN1::INTEGER = 2.
  #
  # == Example usage
  #
  # === Decoding and viewing a DER-encoded file
  #   require 'openssl'
  #   require 'pp'
  #   der = File.binread('data.der')
  #   asn1 = OpenSSL::ASN1.decode(der)
  #   pp der
  #
  # === Creating an ASN.1 structure and DER-encoding it
  #   require 'openssl'
  #   version = OpenSSL::ASN1::Integer.new(1)
  #   # Explicitly 0-tagged implies context-specific tag class
  #   serial = OpenSSL::ASN1::Integer.new(12345, 0, :EXPLICIT, :CONTEXT_SPECIFIC)
  #   name = OpenSSL::ASN1::PrintableString.new('Data 1')
  #   sequence = OpenSSL::ASN1::Sequence.new( [ version, serial, name ] )
  #   der = sequence.to_der
  module ASN1
    # Array storing tag names at the tag's index.
    UNIVERSAL_TAG_NAME = _

    # Decodes a BER- or DER-encoded value and creates an ASN1Data instance. _der_
    # may be a String or any object that features a +.to_der+ method transforming
    # it into a BER-/DER-encoded String+
    #
    # == Example
    #   der = File.binread('asn1data')
    #   asn1 = OpenSSL::ASN1.decode(der)
    def self.decode(der) end

    # Similar to #decode with the difference that #decode expects one
    # distinct value represented in _der_. #decode_all on the contrary
    # decodes a sequence of sequential BER/DER values lined up in _der_
    # and returns them as an array.
    #
    # == Example
    #   ders = File.binread('asn1data_seq')
    #   asn1_ary = OpenSSL::ASN1.decode_all(ders)
    def self.decode_all(der) end

    # If a block is given, it prints out each of the elements encountered.
    # Block parameters are (in that order):
    # * depth: The recursion depth, plus one with each constructed value being encountered (Integer)
    # * offset: Current byte offset (Integer)
    # * header length: Combined length in bytes of the Tag and Length headers. (Integer)
    # * length: The overall remaining length of the entire data (Integer)
    # * constructed: Whether this value is constructed or not (Boolean)
    # * tag_class: Current tag class (Symbol)
    # * tag: The current tag number (Integer)
    #
    # == Example
    #   der = File.binread('asn1data.der')
    #   OpenSSL::ASN1.traverse(der) do | depth, offset, header_len, length, constructed, tag_class, tag|
    #     puts "Depth: #{depth} Offset: #{offset} Length: #{length}"
    #     puts "Header length: #{header_len} Tag: #{tag} Tag class: #{tag_class} Constructed: #{constructed}"
    #   end
    def self.traverse(asn1) end

    private

    # Decodes a BER- or DER-encoded value and creates an ASN1Data instance. _der_
    # may be a String or any object that features a +.to_der+ method transforming
    # it into a BER-/DER-encoded String+
    #
    # == Example
    #   der = File.binread('asn1data')
    #   asn1 = OpenSSL::ASN1.decode(der)
    def decode(der) end

    # Similar to #decode with the difference that #decode expects one
    # distinct value represented in _der_. #decode_all on the contrary
    # decodes a sequence of sequential BER/DER values lined up in _der_
    # and returns them as an array.
    #
    # == Example
    #   ders = File.binread('asn1data_seq')
    #   asn1_ary = OpenSSL::ASN1.decode_all(ders)
    def decode_all(der) end

    # If a block is given, it prints out each of the elements encountered.
    # Block parameters are (in that order):
    # * depth: The recursion depth, plus one with each constructed value being encountered (Integer)
    # * offset: Current byte offset (Integer)
    # * header length: Combined length in bytes of the Tag and Length headers. (Integer)
    # * length: The overall remaining length of the entire data (Integer)
    # * constructed: Whether this value is constructed or not (Boolean)
    # * tag_class: Current tag class (Symbol)
    # * tag: The current tag number (Integer)
    #
    # == Example
    #   der = File.binread('asn1data.der')
    #   OpenSSL::ASN1.traverse(der) do | depth, offset, header_len, length, constructed, tag_class, tag|
    #     puts "Depth: #{depth} Offset: #{offset} Length: #{length}"
    #     puts "Header length: #{header_len} Tag: #{tag} Tag class: #{tag_class} Constructed: #{constructed}"
    #   end
    def traverse(asn1) end

    # The top-level class representing any ASN.1 object. When parsed by
    # ASN1.decode, tagged values are always represented by an instance
    # of ASN1Data.
    #
    # == The role of ASN1Data for parsing tagged values
    #
    # When encoding an ASN.1 type it is inherently clear what original
    # type (e.g. INTEGER, OCTET STRING etc.) this value has, regardless
    # of its tagging.
    # But opposed to the time an ASN.1 type is to be encoded, when parsing
    # them it is not possible to deduce the "real type" of tagged
    # values. This is why tagged values are generally parsed into ASN1Data
    # instances, but with a different outcome for implicit and explicit
    # tagging.
    #
    # === Example of a parsed implicitly tagged value
    #
    # An implicitly 1-tagged INTEGER value will be parsed as an
    # ASN1Data with
    # * _tag_ equal to 1
    # * _tag_class_ equal to +:CONTEXT_SPECIFIC+
    # * _value_ equal to a String that carries the raw encoding
    #   of the INTEGER.
    # This implies that a subsequent decoding step is required to
    # completely decode implicitly tagged values.
    #
    # === Example of a parsed explicitly tagged value
    #
    # An explicitly 1-tagged INTEGER value will be parsed as an
    # ASN1Data with
    # * _tag_ equal to 1
    # * _tag_class_ equal to +:CONTEXT_SPECIFIC+
    # * _value_ equal to an Array with one single element, an
    #   instance of OpenSSL::ASN1::Integer, i.e. the inner element
    #   is the non-tagged primitive value, and the tagging is represented
    #   in the outer ASN1Data
    #
    # == Example - Decoding an implicitly tagged INTEGER
    #   int = OpenSSL::ASN1::Integer.new(1, 0, :IMPLICIT) # implicit 0-tagged
    #   seq = OpenSSL::ASN1::Sequence.new( [int] )
    #   der = seq.to_der
    #   asn1 = OpenSSL::ASN1.decode(der)
    #   # pp asn1 => #<OpenSSL::ASN1::Sequence:0x87326e0
    #   #              @indefinite_length=false,
    #   #              @tag=16,
    #   #              @tag_class=:UNIVERSAL,
    #   #              @tagging=nil,
    #   #              @value=
    #   #                [#<OpenSSL::ASN1::ASN1Data:0x87326f4
    #   #                   @indefinite_length=false,
    #   #                   @tag=0,
    #   #                   @tag_class=:CONTEXT_SPECIFIC,
    #   #                   @value="\x01">]>
    #   raw_int = asn1.value[0]
    #   # manually rewrite tag and tag class to make it an UNIVERSAL value
    #   raw_int.tag = OpenSSL::ASN1::INTEGER
    #   raw_int.tag_class = :UNIVERSAL
    #   int2 = OpenSSL::ASN1.decode(raw_int)
    #   puts int2.value # => 1
    #
    # == Example - Decoding an explicitly tagged INTEGER
    #   int = OpenSSL::ASN1::Integer.new(1, 0, :EXPLICIT) # explicit 0-tagged
    #   seq = OpenSSL::ASN1::Sequence.new( [int] )
    #   der = seq.to_der
    #   asn1 = OpenSSL::ASN1.decode(der)
    #   # pp asn1 => #<OpenSSL::ASN1::Sequence:0x87326e0
    #   #              @indefinite_length=false,
    #   #              @tag=16,
    #   #              @tag_class=:UNIVERSAL,
    #   #              @tagging=nil,
    #   #              @value=
    #   #                [#<OpenSSL::ASN1::ASN1Data:0x87326f4
    #   #                   @indefinite_length=false,
    #   #                   @tag=0,
    #   #                   @tag_class=:CONTEXT_SPECIFIC,
    #   #                   @value=
    #   #                     [#<OpenSSL::ASN1::Integer:0x85bf308
    #   #                        @indefinite_length=false,
    #   #                        @tag=2,
    #   #                        @tag_class=:UNIVERSAL
    #   #                        @tagging=nil,
    #   #                        @value=1>]>]>
    #   int2 = asn1.value[0].value[0]
    #   puts int2.value # => 1
    class ASN1Data
      # _value_: Please have a look at Constructive and Primitive to see how Ruby
      # types are mapped to ASN.1 types and vice versa.
      #
      # _tag_: An Integer indicating the tag number.
      #
      # _tag_class_: A Symbol indicating the tag class. Please cf. ASN1 for
      # possible values.
      #
      # == Example
      #   asn1_int = OpenSSL::ASN1Data.new(42, 2, :UNIVERSAL) # => Same as OpenSSL::ASN1::Integer.new(42)
      #   tagged_int = OpenSSL::ASN1Data.new(42, 0, :CONTEXT_SPECIFIC) # implicitly 0-tagged INTEGER
      def initialize(value, tag, tag_class) end

      # Encodes this ASN1Data into a DER-encoded String value. The result is
      # DER-encoded except for the possibility of indefinite length forms.
      # Indefinite length forms are not allowed in strict DER, so strictly speaking
      # the result of such an encoding would be a BER-encoding.
      def to_der; end
    end

    # Generic error class for all errors raised in ASN1 and any of the
    # classes defined in it.
    class ASN1Error < OpenSSLError
    end

    # The parent class for all constructed encodings. The _value_ attribute
    # of a Constructive is always an Array. Attributes are the same as
    # for ASN1Data, with the addition of _tagging_.
    #
    # == SET and SEQUENCE
    #
    # Most constructed encodings come in the form of a SET or a SEQUENCE.
    # These encodings are represented by one of the two sub-classes of
    # Constructive:
    # * OpenSSL::ASN1::Set
    # * OpenSSL::ASN1::Sequence
    # Please note that tagged sequences and sets are still parsed as
    # instances of ASN1Data. Find further details on tagged values
    # there.
    #
    # === Example - constructing a SEQUENCE
    #   int = OpenSSL::ASN1::Integer.new(1)
    #   str = OpenSSL::ASN1::PrintableString.new('abc')
    #   sequence = OpenSSL::ASN1::Sequence.new( [ int, str ] )
    #
    # === Example - constructing a SET
    #   int = OpenSSL::ASN1::Integer.new(1)
    #   str = OpenSSL::ASN1::PrintableString.new('abc')
    #   set = OpenSSL::ASN1::Set.new( [ int, str ] )
    class Constructive < ASN1Data
      include Enumerable

      # _value_: is mandatory.
      #
      # _tag_: optional, may be specified for tagged values. If no _tag_ is
      # specified, the UNIVERSAL tag corresponding to the Primitive sub-class
      # is used by default.
      #
      # _tagging_: may be used as an encoding hint to encode a value either
      # explicitly or implicitly, see ASN1 for possible values.
      #
      # _tag_class_: if _tag_ and _tagging_ are +nil+ then this is set to
      # +:UNIVERSAL+ by default. If either _tag_ or _tagging_ are set then
      # +:CONTEXT_SPECIFIC+ is used as the default. For possible values please
      # cf. ASN1.
      #
      # == Example
      #   int = OpenSSL::ASN1::Integer.new(42)
      #   zero_tagged_int = OpenSSL::ASN1::Integer.new(42, 0, :IMPLICIT)
      #   private_explicit_zero_tagged_int = OpenSSL::ASN1::Integer.new(42, 0, :EXPLICIT, :PRIVATE)
      def initialize(p1, p2 = v2, p3 = v3, p4 = v4) end

      # Calls the given block once for each element in self, passing that element
      # as parameter _asn1_. If no block is given, an enumerator is returned
      # instead.
      #
      # == Example
      #   asn1_ary.each do |asn1|
      #     puts asn1
      #   end
      def each; end

      # See ASN1Data#to_der for details.
      def to_der; end
    end

    # Represents the primitive object id for OpenSSL::ASN1
    class ObjectId < Primitive
      # This adds a new ObjectId to the internal tables. Where _object_id_ is the
      # numerical form, _short_name_ is the short name, and _long_name_ is the long
      # name.
      #
      # Returns +true+ if successful. Raises an OpenSSL::ASN1::ASN1Error if it fails.
      def self.register(object_id, short_name, long_name) end

      # The long name of the ObjectId, as defined in <openssl/objects.h>.
      def ln; end
      alias long_name ln

      # Returns a String representing the Object Identifier in the dot notation,
      # e.g. "1.2.3.4.5"
      def oid; end

      # The short name of the ObjectId, as defined in <openssl/objects.h>.
      def sn; end
      alias short_name sn
    end

    # The parent class for all primitive encodings. Attributes are the same as
    # for ASN1Data, with the addition of _tagging_.
    # Primitive values can never be encoded with indefinite length form, thus
    # it is not possible to set the _indefinite_length_ attribute for Primitive
    # and its sub-classes.
    #
    # == Primitive sub-classes and their mapping to Ruby classes
    # * OpenSSL::ASN1::EndOfContent    <=> _value_ is always +nil+
    # * OpenSSL::ASN1::Boolean         <=> _value_ is +true+ or +false+
    # * OpenSSL::ASN1::Integer         <=> _value_ is an OpenSSL::BN
    # * OpenSSL::ASN1::BitString       <=> _value_ is a String
    # * OpenSSL::ASN1::OctetString     <=> _value_ is a String
    # * OpenSSL::ASN1::Null            <=> _value_ is always +nil+
    # * OpenSSL::ASN1::Object          <=> _value_ is a String
    # * OpenSSL::ASN1::Enumerated      <=> _value_ is an OpenSSL::BN
    # * OpenSSL::ASN1::UTF8String      <=> _value_ is a String
    # * OpenSSL::ASN1::NumericString   <=> _value_ is a String
    # * OpenSSL::ASN1::PrintableString <=> _value_ is a String
    # * OpenSSL::ASN1::T61String       <=> _value_ is a String
    # * OpenSSL::ASN1::VideotexString  <=> _value_ is a String
    # * OpenSSL::ASN1::IA5String       <=> _value_ is a String
    # * OpenSSL::ASN1::UTCTime         <=> _value_ is a Time
    # * OpenSSL::ASN1::GeneralizedTime <=> _value_ is a Time
    # * OpenSSL::ASN1::GraphicString   <=> _value_ is a String
    # * OpenSSL::ASN1::ISO64String     <=> _value_ is a String
    # * OpenSSL::ASN1::GeneralString   <=> _value_ is a String
    # * OpenSSL::ASN1::UniversalString <=> _value_ is a String
    # * OpenSSL::ASN1::BMPString       <=> _value_ is a String
    #
    # == OpenSSL::ASN1::BitString
    #
    # === Additional attributes
    # _unused_bits_: if the underlying BIT STRING's
    # length is a multiple of 8 then _unused_bits_ is 0. Otherwise
    # _unused_bits_ indicates the number of bits that are to be ignored in
    # the final octet of the BitString's _value_.
    #
    # == OpenSSL::ASN1::ObjectId
    #
    # NOTE: While OpenSSL::ASN1::ObjectId.new will allocate a new ObjectId,
    # it is not typically allocated this way, but rather that are received from
    # parsed ASN1 encodings.
    #
    # === Additional attributes
    # * _sn_: the short name as defined in <openssl/objects.h>.
    # * _ln_: the long name as defined in <openssl/objects.h>.
    # * _oid_: the object identifier as a String, e.g. "1.2.3.4.5"
    # * _short_name_: alias for _sn_.
    # * _long_name_: alias for _ln_.
    #
    # == Examples
    # With the Exception of OpenSSL::ASN1::EndOfContent, each Primitive class
    # constructor takes at least one parameter, the _value_.
    #
    # === Creating EndOfContent
    #   eoc = OpenSSL::ASN1::EndOfContent.new
    #
    # === Creating any other Primitive
    #   prim = <class>.new(value) # <class> being one of the sub-classes except EndOfContent
    #   prim_zero_tagged_implicit = <class>.new(value, 0, :IMPLICIT)
    #   prim_zero_tagged_explicit = <class>.new(value, 0, :EXPLICIT)
    class Primitive < ASN1Data
      # _value_: is mandatory.
      #
      # _tag_: optional, may be specified for tagged values. If no _tag_ is
      # specified, the UNIVERSAL tag corresponding to the Primitive sub-class
      # is used by default.
      #
      # _tagging_: may be used as an encoding hint to encode a value either
      # explicitly or implicitly, see ASN1 for possible values.
      #
      # _tag_class_: if _tag_ and _tagging_ are +nil+ then this is set to
      # +:UNIVERSAL+ by default. If either _tag_ or _tagging_ are set then
      # +:CONTEXT_SPECIFIC+ is used as the default. For possible values please
      # cf. ASN1.
      #
      # == Example
      #   int = OpenSSL::ASN1::Integer.new(42)
      #   zero_tagged_int = OpenSSL::ASN1::Integer.new(42, 0, :IMPLICIT)
      #   private_explicit_zero_tagged_int = OpenSSL::ASN1::Integer.new(42, 0, :EXPLICIT, :PRIVATE)
      def initialize(p1, p2 = v2, p3 = v3, p4 = v4) end

      # See ASN1Data#to_der for details.
      def to_der; end
    end
  end

  class BN
    # Generates a random prime number of bit length _bits_. If _safe_ is set to
    # +true+, generates a safe prime. If _add_ is specified, generates a prime that
    # fulfills condition <tt>p % add = rem</tt>.
    #
    # === Parameters
    # * _bits_ - integer
    # * _safe_ - boolean
    # * _add_ - BN
    # * _rem_ - BN
    def self.generate_prime(p1, p2 = v2, p3 = v3, p4 = v4) end

    # Construct a new OpenSSL BIGNUM object.
    def initialize(*several_variants) end

    def %(other) end

    def *(other) end

    def **(other) end

    def +(other) end

    def +@; end

    def -(other) end

    def -@; end

    # Division of OpenSSL::BN instances
    def /(other) end

    def <<(bits) end

    # Returns +true+ only if _obj_ has the same value as _bn_. Contrast this
    # with OpenSSL::BN#eql?, which requires obj to be OpenSSL::BN.
    def ==(other) end
    alias === ==

    def >>(other) end

    # Tests bit _bit_ in _bn_ and returns +true+ if set, +false+ if not set.
    def bit_set?(bit) end

    def clear_bit!(bit) end

    def cmp(bn2) end
    alias <=> cmp

    def coerce(p1) end

    # Returns <code>true</code> only if <i>obj</i> is a
    # <code>OpenSSL::BN</code> with the same value as <i>bn</i>. Contrast this
    # with OpenSSL::BN#==, which performs type conversions.
    def eql?(other) end

    def gcd(bn2) end

    # Returns a hash code for this object.
    #
    # See also Object#hash.
    def hash; end

    def initialize_copy(p1) end
    alias copy initialize_copy

    def lshift!(bits) end

    def mod_add(bn1, bn2) end

    def mod_exp(bn1, bn2) end

    def mod_inverse(bn2) end

    def mod_mul(bn1, bn2) end

    def mod_sqr(bn2) end

    def mod_sub(bn1, bn2) end

    def negative?; end

    def num_bits; end

    def num_bytes; end

    def odd?; end

    def one?; end

    # Performs a Miller-Rabin probabilistic primality test with _checks_
    # iterations. If _checks_ is not specified, a number of iterations is used
    # that yields a false positive rate of at most 2^-80 for random input.
    #
    # === Parameters
    # * _checks_ - integer
    def prime?(*several_variants) end

    # Performs a Miller-Rabin primality test. This is same as #prime? except this
    # first attempts trial divisions with some small primes.
    #
    # === Parameters
    # * _checks_ - integer
    # * _trial_div_ - boolean
    def prime_fasttest?(*several_variants) end

    def rshift!(bits) end

    def set_bit!(bit) end

    def sqr; end

    def to_bn; end

    def to_i; end
    alias to_int to_i

    # === Parameters
    # * _base_ - Integer
    #   Valid values:
    #   * 0 - MPI
    #   * 2 - binary
    #   * 10 - the default
    #   * 16 - hex
    def to_s(*several_variants) end

    def ucmp(bn2) end

    def zero?; end
  end

  # Generic Error for all of OpenSSL::BN (big num)
  class BNError < OpenSSLError
  end

  # Provides symmetric algorithms for encryption and decryption. The
  # algorithms that are available depend on the particular version
  # of OpenSSL that is installed.
  #
  # === Listing all supported algorithms
  #
  # A list of supported algorithms can be obtained by
  #
  #   puts OpenSSL::Cipher.ciphers
  #
  # === Instantiating a Cipher
  #
  # There are several ways to create a Cipher instance. Generally, a
  # Cipher algorithm is categorized by its name, the key length in bits
  # and the cipher mode to be used. The most generic way to create a
  # Cipher is the following
  #
  #   cipher = OpenSSL::Cipher.new('<name>-<key length>-<mode>')
  #
  # That is, a string consisting of the hyphenated concatenation of the
  # individual components name, key length and mode. Either all uppercase
  # or all lowercase strings may be used, for example:
  #
  #  cipher = OpenSSL::Cipher.new('AES-128-CBC')
  #
  # For each algorithm supported, there is a class defined under the
  # Cipher class that goes by the name of the cipher, e.g. to obtain an
  # instance of AES, you could also use
  #
  #   # these are equivalent
  #   cipher = OpenSSL::Cipher::AES.new(128, :CBC)
  #   cipher = OpenSSL::Cipher::AES.new(128, 'CBC')
  #   cipher = OpenSSL::Cipher::AES.new('128-CBC')
  #
  # Finally, due to its wide-spread use, there are also extra classes
  # defined for the different key sizes of AES
  #
  #   cipher = OpenSSL::Cipher::AES128.new(:CBC)
  #   cipher = OpenSSL::Cipher::AES192.new(:CBC)
  #   cipher = OpenSSL::Cipher::AES256.new(:CBC)
  #
  # === Choosing either encryption or decryption mode
  #
  # Encryption and decryption are often very similar operations for
  # symmetric algorithms, this is reflected by not having to choose
  # different classes for either operation, both can be done using the
  # same class. Still, after obtaining a Cipher instance, we need to
  # tell the instance what it is that we intend to do with it, so we
  # need to call either
  #
  #   cipher.encrypt
  #
  # or
  #
  #   cipher.decrypt
  #
  # on the Cipher instance. This should be the first call after creating
  # the instance, otherwise configuration that has already been set could
  # get lost in the process.
  #
  # === Choosing a key
  #
  # Symmetric encryption requires a key that is the same for the encrypting
  # and for the decrypting party and after initial key establishment should
  # be kept as private information. There are a lot of ways to create
  # insecure keys, the most notable is to simply take a password as the key
  # without processing the password further. A simple and secure way to
  # create a key for a particular Cipher is
  #
  #  cipher = OpenSSL::AES256.new(:CFB)
  #  cipher.encrypt
  #  key = cipher.random_key # also sets the generated key on the Cipher
  #
  # If you absolutely need to use passwords as encryption keys, you
  # should use Password-Based Key Derivation Function 2 (PBKDF2) by
  # generating the key with the help of the functionality provided by
  # OpenSSL::PKCS5.pbkdf2_hmac_sha1 or OpenSSL::PKCS5.pbkdf2_hmac.
  #
  # Although there is Cipher#pkcs5_keyivgen, its use is deprecated and
  # it should only be used in legacy applications because it does not use
  # the newer PKCS#5 v2 algorithms.
  #
  # === Choosing an IV
  #
  # The cipher modes CBC, CFB, OFB and CTR all need an "initialization
  # vector", or short, IV. ECB mode is the only mode that does not require
  # an IV, but there is almost no legitimate use case for this mode
  # because of the fact that it does not sufficiently hide plaintext
  # patterns. Therefore
  #
  # <b>You should never use ECB mode unless you are absolutely sure that
  # you absolutely need it</b>
  #
  # Because of this, you will end up with a mode that explicitly requires
  # an IV in any case. Although the IV can be seen as public information,
  # i.e. it may be transmitted in public once generated, it should still
  # stay unpredictable to prevent certain kinds of attacks. Therefore,
  # ideally
  #
  # <b>Always create a secure random IV for every encryption of your
  # Cipher</b>
  #
  # A new, random IV should be created for every encryption of data. Think
  # of the IV as a nonce (number used once) - it's public but random and
  # unpredictable. A secure random IV can be created as follows
  #
  #   cipher = ...
  #   cipher.encrypt
  #   key = cipher.random_key
  #   iv = cipher.random_iv # also sets the generated IV on the Cipher
  #
  # Although the key is generally a random value, too, it is a bad choice
  # as an IV. There are elaborate ways how an attacker can take advantage
  # of such an IV. As a general rule of thumb, exposing the key directly
  # or indirectly should be avoided at all cost and exceptions only be
  # made with good reason.
  #
  # === Calling Cipher#final
  #
  # ECB (which should not be used) and CBC are both block-based modes.
  # This means that unlike for the other streaming-based modes, they
  # operate on fixed-size blocks of data, and therefore they require a
  # "finalization" step to produce or correctly decrypt the last block of
  # data by appropriately handling some form of padding. Therefore it is
  # essential to add the output of OpenSSL::Cipher#final to your
  # encryption/decryption buffer or you will end up with decryption errors
  # or truncated data.
  #
  # Although this is not really necessary for streaming-mode ciphers, it is
  # still recommended to apply the same pattern of adding the output of
  # Cipher#final there as well - it also enables you to switch between
  # modes more easily in the future.
  #
  # === Encrypting and decrypting some data
  #
  #   data = "Very, very confidential data"
  #
  #   cipher = OpenSSL::Cipher::AES.new(128, :CBC)
  #   cipher.encrypt
  #   key = cipher.random_key
  #   iv = cipher.random_iv
  #
  #   encrypted = cipher.update(data) + cipher.final
  #   ...
  #   decipher = OpenSSL::Cipher::AES.new(128, :CBC)
  #   decipher.decrypt
  #   decipher.key = key
  #   decipher.iv = iv
  #
  #   plain = decipher.update(encrypted) + decipher.final
  #
  #   puts data == plain #=> true
  #
  # === Authenticated Encryption and Associated Data (AEAD)
  #
  # If the OpenSSL version used supports it, an Authenticated Encryption
  # mode (such as GCM or CCM) should always be preferred over any
  # unauthenticated mode. Currently, OpenSSL supports AE only in combination
  # with Associated Data (AEAD) where additional associated data is included
  # in the encryption process to compute a tag at the end of the encryption.
  # This tag will also be used in the decryption process and by verifying
  # its validity, the authenticity of a given ciphertext is established.
  #
  # This is superior to unauthenticated modes in that it allows to detect
  # if somebody effectively changed the ciphertext after it had been
  # encrypted. This prevents malicious modifications of the ciphertext that
  # could otherwise be exploited to modify ciphertexts in ways beneficial to
  # potential attackers.
  #
  # An associated data is used where there is additional information, such as
  # headers or some metadata, that must be also authenticated but not
  # necessarily need to be encrypted. If no associated data is needed for
  # encryption and later decryption, the OpenSSL library still requires a
  # value to be set - "" may be used in case none is available.
  #
  # An example using the GCM (Galois/Counter Mode). You have 16 bytes _key_,
  # 12 bytes (96 bits) _nonce_ and the associated data _auth_data_. Be sure
  # not to reuse the _key_ and _nonce_ pair. Reusing an nonce ruins the
  # security guarantees of GCM mode.
  #
  #   cipher = OpenSSL::Cipher::AES.new(128, :GCM).encrypt
  #   cipher.key = key
  #   cipher.iv = nonce
  #   cipher.auth_data = auth_data
  #
  #   encrypted = cipher.update(data) + cipher.final
  #   tag = cipher.auth_tag # produces 16 bytes tag by default
  #
  # Now you are the receiver. You know the _key_ and have received _nonce_,
  # _auth_data_, _encrypted_ and _tag_ through an untrusted network. Note
  # that GCM accepts an arbitrary length tag between 1 and 16 bytes. You may
  # additionally need to check that the received tag has the correct length,
  # or you allow attackers to forge a valid single byte tag for the tampered
  # ciphertext with a probability of 1/256.
  #
  #   raise "tag is truncated!" unless tag.bytesize == 16
  #   decipher = OpenSSL::Cipher::AES.new(128, :GCM).decrypt
  #   decipher.key = key
  #   decipher.iv = nonce
  #   decipher.auth_tag = tag
  #   decipher.auth_data = auth_data
  #
  #   decrypted = decipher.update(encrypted) + decipher.final
  #
  #   puts data == decrypted #=> true
  class Cipher
    # Returns the names of all available ciphers in an array.
    def self.ciphers; end

    # The string must be a valid cipher name like "AES-128-CBC" or "3DES".
    #
    # A list of cipher names is available by calling OpenSSL::Cipher.ciphers.
    def initialize(string) end

    # Sets the cipher's additional authenticated data. This field must be
    # set when using AEAD cipher modes such as GCM or CCM. If no associated
    # data shall be used, this method must *still* be called with a value of "".
    # The contents of this field should be non-sensitive data which will be
    # added to the ciphertext to generate the authentication tag which validates
    # the contents of the ciphertext.
    #
    # The AAD must be set prior to encryption or decryption. In encryption mode,
    # it must be set after calling Cipher#encrypt and setting Cipher#key= and
    # Cipher#iv=. When decrypting, the authenticated data must be set after key,
    # iv and especially *after* the authentication tag has been set. I.e. set it
    # only after calling Cipher#decrypt, Cipher#key=, Cipher#iv= and
    # Cipher#auth_tag= first.
    def auth_data=(string) end

    # Gets the authentication tag generated by Authenticated Encryption Cipher
    # modes (GCM for example). This tag may be stored along with the ciphertext,
    # then set on the decryption cipher to authenticate the contents of the
    # ciphertext against changes. If the optional integer parameter _tag_len_ is
    # given, the returned tag will be _tag_len_ bytes long. If the parameter is
    # omitted, the default length of 16 bytes or the length previously set by
    # #auth_tag_len= will be used. For maximum security, the longest possible
    # should be chosen.
    #
    # The tag may only be retrieved after calling Cipher#final.
    def auth_tag(tag_len = 16) end

    # Sets the authentication tag to verify the integrity of the ciphertext.
    # This can be called only when the cipher supports AE. The tag must be set
    # after calling Cipher#decrypt, Cipher#key= and Cipher#iv=, but before
    # calling Cipher#final. After all decryption is performed, the tag is
    # verified automatically in the call to Cipher#final.
    #
    # For OCB mode, the tag length must be supplied with #auth_tag_len=
    # beforehand.
    def auth_tag=(string) end

    # Sets the length of the authentication tag to be generated or to be given for
    # AEAD ciphers that requires it as in input parameter. Note that not all AEAD
    # ciphers support this method.
    #
    # In OCB mode, the length must be supplied both when encrypting and when
    # decrypting, and must be before specifying an IV.
    def auth_tag_len=(p1) end

    # Indicated whether this Cipher instance uses an Authenticated Encryption
    # mode.
    def authenticated?; end

    # Returns the size in bytes of the blocks on which this Cipher operates on.
    def block_size; end

    # Initializes the Cipher for decryption.
    #
    # Make sure to call Cipher#encrypt or Cipher#decrypt before using any of the
    # following methods:
    # * [#key=, #iv=, #random_key, #random_iv, #pkcs5_keyivgen]
    #
    # Internally calls EVP_CipherInit_ex(ctx, NULL, NULL, NULL, NULL, 0).
    def decrypt; end

    # Initializes the Cipher for encryption.
    #
    # Make sure to call Cipher#encrypt or Cipher#decrypt before using any of the
    # following methods:
    # * [#key=, #iv=, #random_key, #random_iv, #pkcs5_keyivgen]
    #
    # Internally calls EVP_CipherInit_ex(ctx, NULL, NULL, NULL, NULL, 1).
    def encrypt; end

    # Returns the remaining data held in the cipher object. Further calls to
    # Cipher#update or Cipher#final will return garbage. This call should always
    # be made as the last call of an encryption or decryption operation, after
    # having fed the entire plaintext or ciphertext to the Cipher instance.
    #
    # If an authenticated cipher was used, a CipherError is raised if the tag
    # could not be authenticated successfully. Only call this method after
    # setting the authentication tag and passing the entire contents of the
    # ciphertext into the cipher.
    def final; end

    def initialize_copy(p1) end

    # Sets the cipher IV. Please note that since you should never be using ECB
    # mode, an IV is always explicitly required and should be set prior to
    # encryption. The IV itself can be safely transmitted in public, but it
    # should be unpredictable to prevent certain kinds of attacks. You may use
    # Cipher#random_iv to create a secure random IV.
    #
    # Only call this method after calling Cipher#encrypt or Cipher#decrypt.
    def iv=(string) end

    # Returns the expected length in bytes for an IV for this Cipher.
    def iv_len; end

    # Sets the IV/nonce length of the Cipher. Normally block ciphers don't allow
    # changing the IV length, but some make use of IV for 'nonce'. You may need
    # this for interoperability with other applications.
    def iv_len=(integer) end

    # Sets the cipher key. To generate a key, you should either use a secure
    # random byte string or, if the key is to be derived from a password, you
    # should rely on PBKDF2 functionality provided by OpenSSL::PKCS5. To
    # generate a secure random-based key, Cipher#random_key may be used.
    #
    # Only call this method after calling Cipher#encrypt or Cipher#decrypt.
    def key=(string) end

    # Returns the key length in bytes of the Cipher.
    def key_len; end

    # Sets the key length of the cipher.  If the cipher is a fixed length cipher
    # then attempting to set the key length to any value other than the fixed
    # value is an error.
    #
    # Under normal circumstances you do not need to call this method (and probably shouldn't).
    #
    # See EVP_CIPHER_CTX_set_key_length for further information.
    def key_len=(integer) end

    # Returns the name of the cipher which may differ slightly from the original
    # name provided.
    def name; end

    # Enables or disables padding. By default encryption operations are padded using standard block padding and the
    # padding is checked and removed when decrypting. If the pad parameter is zero then no padding is performed, the
    # total amount of data encrypted or decrypted must then be a multiple of the block size or an error will occur.
    #
    # See EVP_CIPHER_CTX_set_padding for further information.
    def padding=(integer) end

    # Generates and sets the key/IV based on a password.
    #
    # *WARNING*: This method is only PKCS5 v1.5 compliant when using RC2, RC4-40,
    # or DES with MD5 or SHA1. Using anything else (like AES) will generate the
    # key/iv using an OpenSSL specific method. This method is deprecated and
    # should no longer be used. Use a PKCS5 v2 key generation method from
    # OpenSSL::PKCS5 instead.
    #
    # === Parameters
    # * _salt_ must be an 8 byte string if provided.
    # * _iterations_ is an integer with a default of 2048.
    # * _digest_ is a Digest object that defaults to 'MD5'
    #
    # A minimum of 1000 iterations is recommended.
    def pkcs5_keyivgen(pass, salt = nil, iterations = 2048, digest = 'MD5') end

    # Fully resets the internal state of the Cipher. By using this, the same
    # Cipher instance may be used several times for encryption or decryption tasks.
    #
    # Internally calls EVP_CipherInit_ex(ctx, NULL, NULL, NULL, NULL, -1).
    def reset; end

    # Encrypts data in a streaming fashion. Hand consecutive blocks of data
    # to the #update method in order to encrypt it. Returns the encrypted
    # data chunk. When done, the output of Cipher#final should be additionally
    # added to the result.
    #
    # If _buffer_ is given, the encryption/decryption result will be written to
    # it. _buffer_ will be resized automatically.
    def update(p1, p2 = v2) end

    private

    # Returns the names of all available ciphers in an array.
    def ciphers; end

    class CipherError < OpenSSLError
    end
  end

  class Config
    # The default system configuration file for openssl
    DEFAULT_CONFIG_FILE = _
  end

  # General error for openssl library configuration files. Including formatting,
  # parsing errors, etc.
  class ConfigError < OpenSSLError
  end

  # OpenSSL::Digest allows you to compute message digests (sometimes
  # interchangeably called "hashes") of arbitrary data that are
  # cryptographically secure, i.e. a Digest implements a secure one-way
  # function.
  #
  # One-way functions offer some useful properties. E.g. given two
  # distinct inputs the probability that both yield the same output
  # is highly unlikely. Combined with the fact that every message digest
  # algorithm has a fixed-length output of just a few bytes, digests are
  # often used to create unique identifiers for arbitrary data. A common
  # example is the creation of a unique id for binary documents that are
  # stored in a database.
  #
  # Another useful characteristic of one-way functions (and thus the name)
  # is that given a digest there is no indication about the original
  # data that produced it, i.e. the only way to identify the original input
  # is to "brute-force" through every possible combination of inputs.
  #
  # These characteristics make one-way functions also ideal companions
  # for public key signature algorithms: instead of signing an entire
  # document, first a hash of the document is produced with a considerably
  # faster message digest algorithm and only the few bytes of its output
  # need to be signed using the slower public key algorithm. To validate
  # the integrity of a signed document, it suffices to re-compute the hash
  # and verify that it is equal to that in the signature.
  #
  # Among the supported message digest algorithms are:
  # * SHA, SHA1, SHA224, SHA256, SHA384 and SHA512
  # * MD2, MD4, MDC2 and MD5
  # * RIPEMD160
  # * DSS, DSS1 (Pseudo algorithms to be used for DSA signatures. DSS is
  #   equal to SHA and DSS1 is equal to SHA1)
  #
  # For each of these algorithms, there is a sub-class of Digest that
  # can be instantiated as simply as e.g.
  #
  #   digest = OpenSSL::Digest::SHA1.new
  #
  # === Mapping between Digest class and sn/ln
  #
  # The sn (short names) and ln (long names) are defined in
  # <openssl/object.h> and <openssl/obj_mac.h>. They are textual
  # representations of ASN.1 OBJECT IDENTIFIERs. Each supported digest
  # algorithm has an OBJECT IDENTIFIER associated to it and those again
  # have short/long names assigned to them.
  # E.g. the OBJECT IDENTIFIER for SHA-1 is 1.3.14.3.2.26 and its
  # sn is "SHA1" and its ln is "sha1".
  # ==== MD2
  # * sn: MD2
  # * ln: md2
  # ==== MD4
  # * sn: MD4
  # * ln: md4
  # ==== MD5
  # * sn: MD5
  # * ln: md5
  # ==== SHA
  # * sn: SHA
  # * ln: SHA
  # ==== SHA-1
  # * sn: SHA1
  # * ln: sha1
  # ==== SHA-224
  # * sn: SHA224
  # * ln: sha224
  # ==== SHA-256
  # * sn: SHA256
  # * ln: sha256
  # ==== SHA-384
  # * sn: SHA384
  # * ln: sha384
  # ==== SHA-512
  # * sn: SHA512
  # * ln: sha512
  #
  # "Breaking" a message digest algorithm means defying its one-way
  # function characteristics, i.e. producing a collision or finding a way
  # to get to the original data by means that are more efficient than
  # brute-forcing etc. Most of the supported digest algorithms can be
  # considered broken in this sense, even the very popular MD5 and SHA1
  # algorithms. Should security be your highest concern, then you should
  # probably rely on SHA224, SHA256, SHA384 or SHA512.
  #
  # === Hashing a file
  #
  #   data = File.read('document')
  #   sha256 = OpenSSL::Digest::SHA256.new
  #   digest = sha256.digest(data)
  #
  # === Hashing several pieces of data at once
  #
  #   data1 = File.read('file1')
  #   data2 = File.read('file2')
  #   data3 = File.read('file3')
  #   sha256 = OpenSSL::Digest::SHA256.new
  #   sha256 << data1
  #   sha256 << data2
  #   sha256 << data3
  #   digest = sha256.digest
  #
  # === Reuse a Digest instance
  #
  #   data1 = File.read('file1')
  #   sha256 = OpenSSL::Digest::SHA256.new
  #   digest1 = sha256.digest(data1)
  #
  #   data2 = File.read('file2')
  #   sha256.reset
  #   digest2 = sha256.digest(data2)
  class Digest < Class
    # Creates a Digest instance based on _string_, which is either the ln
    # (long name) or sn (short name) of a supported digest algorithm.
    #
    # If _data_ (a String) is given, it is used as the initial input to the
    # Digest instance, i.e.
    #
    #   digest = OpenSSL::Digest.new('sha256', 'digestdata')
    #
    # is equivalent to
    #
    #   digest = OpenSSL::Digest.new('sha256')
    #   digest.update('digestdata')
    def initialize(p1, p2 = v2) end

    # Returns the block length of the digest algorithm, i.e. the length in bytes
    # of an individual block. Most modern algorithms partition a message to be
    # digested into a sequence of fix-sized blocks that are processed
    # consecutively.
    #
    # === Example
    #   digest = OpenSSL::Digest::SHA1.new
    #   puts digest.block_length # => 64
    def block_length; end

    # Returns the output size of the digest, i.e. the length in bytes of the
    # final message digest result.
    #
    # === Example
    #   digest = OpenSSL::Digest::SHA1.new
    #   puts digest.digest_length # => 20
    def digest_length; end

    def initialize_copy(p1) end

    # Returns the sn of this Digest algorithm.
    #
    # === Example
    #   digest = OpenSSL::Digest::SHA512.new
    #   puts digest.name # => SHA512
    def name; end

    # Resets the Digest in the sense that any Digest#update that has been
    # performed is abandoned and the Digest is set to its initial state again.
    def reset; end

    # Not every message digest can be computed in one single pass. If a message
    # digest is to be computed from several subsequent sources, then each may
    # be passed individually to the Digest instance.
    #
    # === Example
    #   digest = OpenSSL::Digest::SHA256.new
    #   digest.update('First input')
    #   digest << 'Second input' # equivalent to digest.update('Second input')
    #   result = digest.digest
    def update(string) end
    alias << update

    private

    def finish; end

    # Generic Exception class that is raised if an error occurs during a
    # Digest operation.
    class DigestError < OpenSSLError
    end
  end

  # This class is the access to openssl's ENGINE cryptographic module
  # implementation.
  #
  # See also, https://www.openssl.org/docs/crypto/engine.html
  class Engine
    # Fetches the engine as specified by the _id_ String.
    #
    #   OpenSSL::Engine.by_id("openssl")
    #    => #<OpenSSL::Engine id="openssl" name="Software engine support">
    #
    # See OpenSSL::Engine.engines for the currently loaded engines.
    def self.by_id(name) end

    # It is only necessary to run cleanup when engines are loaded via
    # OpenSSL::Engine.load. However, running cleanup before exit is recommended.
    #
    # Note that this is needed and works only in OpenSSL < 1.1.0.
    def self.cleanup; end

    # Returns an array of currently loaded engines.
    def self.engines; end

    # This method loads engines. If _name_ is nil, then all builtin engines are
    # loaded. Otherwise, the given _name_, as a String,  is loaded if available to
    # your runtime, and returns true. If _name_ is not found, then nil is
    # returned.
    def self.load(name = nil) end

    # Returns a new instance of OpenSSL::Cipher by _name_, if it is available in
    # this engine.
    #
    # An EngineError will be raised if the cipher is unavailable.
    #
    #    e = OpenSSL::Engine.by_id("openssl")
    #     => #<OpenSSL::Engine id="openssl" name="Software engine support">
    #    e.cipher("RC4")
    #     => #<OpenSSL::Cipher:0x007fc5cacc3048>
    def cipher(name) end

    # Returns an array of command definitions for the current engine
    def cmds; end

    # Sends the given _command_ to this engine.
    #
    # Raises an EngineError if the command fails.
    def ctrl_cmd(command, value = nil) end

    # Returns a new instance of OpenSSL::Digest by _name_.
    #
    # Will raise an EngineError if the digest is unavailable.
    #
    #    e = OpenSSL::Engine.by_id("openssl")
    #      #=> #<OpenSSL::Engine id="openssl" name="Software engine support">
    #    e.digest("SHA1")
    #      #=> #<OpenSSL::Digest: da39a3ee5e6b4b0d3255bfef95601890afd80709>
    #    e.digest("zomg")
    #      #=> OpenSSL::Engine::EngineError: no such digest `zomg'
    def digest(name) end

    # Releases all internal structural references for this engine.
    #
    # May raise an EngineError if the engine is unavailable
    def finish; end

    # Gets the id for this engine.
    #
    #    OpenSSL::Engine.load
    #    OpenSSL::Engine.engines #=> [#<OpenSSL::Engine#>, ...]
    #    OpenSSL::Engine.engines.first.id
    #      #=> "rsax"
    def id; end

    # Pretty prints this engine.
    def inspect; end

    # Loads the given private key identified by _id_ and _data_.
    #
    # An EngineError is raised of the OpenSSL::PKey is unavailable.
    def load_private_key(id = nil, data = nil) end

    # Loads the given public key identified by _id_ and _data_.
    #
    # An EngineError is raised of the OpenSSL::PKey is unavailable.
    def load_public_key(id = nil, data = nil) end

    # Get the descriptive name for this engine.
    #
    #    OpenSSL::Engine.load
    #    OpenSSL::Engine.engines #=> [#<OpenSSL::Engine#>, ...]
    #    OpenSSL::Engine.engines.first.name
    #      #=> "RSAX engine support"
    def name; end

    # Set the defaults for this engine with the given _flag_.
    #
    # These flags are used to control combinations of algorithm methods.
    #
    # _flag_ can be one of the following, other flags are available depending on
    # your OS.
    #
    # [All flags]  0xFFFF
    # [No flags]   0x0000
    #
    # See also <openssl/engine.h>
    def set_default(flag) end

    # This is the generic exception for OpenSSL::Engine related errors
    class EngineError < OpenSSLError
    end
  end

  # This module contains configuration information about the SSL extension,
  # for example if socket support is enabled, or the host name TLS extension
  # is enabled.  Constants in this module will always be defined, but contain
  # +true+ or +false+ values depending on the configuration of your OpenSSL
  # installation.
  module ExtConfig
    HAVE_TLSEXT_HOST_NAME = _
    OPENSSL_NO_SOCK = _
  end

  # OpenSSL::HMAC allows computing Hash-based Message Authentication Code
  # (HMAC). It is a type of message authentication code (MAC) involving a
  # hash function in combination with a key. HMAC can be used to verify the
  # integrity of a message as well as the authenticity.
  #
  # OpenSSL::HMAC has a similar interface to OpenSSL::Digest.
  #
  # === HMAC-SHA256 using one-shot interface
  #
  #   key = "key"
  #   data = "message-to-be-authenticated"
  #   mac = OpenSSL::HMAC.hexdigest("SHA256", key, data)
  #   #=> "cddb0db23f469c8bf072b21fd837149bd6ace9ab771cceef14c9e517cc93282e"
  #
  # === HMAC-SHA256 using incremental interface
  #
  #   data1 = File.read("file1")
  #   data2 = File.read("file2")
  #   key = "key"
  #   digest = OpenSSL::Digest::SHA256.new
  #   hmac = OpenSSL::HMAC.new(key, digest)
  #   hmac << data1
  #   hmac << data2
  #   mac = hmac.digest
  class HMAC
    # Returns the authentication code as a binary string. The _digest_ parameter
    # specifies the digest algorithm to use. This may be a String representing
    # the algorithm name or an instance of OpenSSL::Digest.
    #
    # === Example
    #
    #      key = 'key'
    #      data = 'The quick brown fox jumps over the lazy dog'
    #
    #      hmac = OpenSSL::HMAC.digest('sha1', key, data)
    #      #=> "\xDE|\x9B\x85\xB8\xB7\x8A\xA6\xBC\x8Az6\xF7\n\x90p\x1C\x9D\xB4\xD9"
    def self.digest(digest, key, data) end

    # Returns the authentication code as a hex-encoded string. The _digest_
    # parameter specifies the digest algorithm to use. This may be a String
    # representing the algorithm name or an instance of OpenSSL::Digest.
    #
    # === Example
    #
    #      key = 'key'
    #      data = 'The quick brown fox jumps over the lazy dog'
    #
    #      hmac = OpenSSL::HMAC.hexdigest('sha1', key, data)
    #      #=> "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9"
    def self.hexdigest(digest, key, data) end

    # Returns an instance of OpenSSL::HMAC set with the key and digest
    # algorithm to be used. The instance represents the initial state of
    # the message authentication code before any data has been processed.
    # To process data with it, use the instance method #update with your
    # data as an argument.
    #
    # === Example
    #
    #      key = 'key'
    #      digest = OpenSSL::Digest.new('sha1')
    #      instance = OpenSSL::HMAC.new(key, digest)
    #      #=> f42bb0eeb018ebbd4597ae7213711ec60760843f
    #      instance.class
    #      #=> OpenSSL::HMAC
    #
    # === A note about comparisons
    #
    # Two instances won't be equal when they're compared, even if they have the
    # same value. Use #to_s or #hexdigest to return the authentication code that
    # the instance represents. For example:
    #
    #      other_instance = OpenSSL::HMAC.new('key', OpenSSL::Digest.new('sha1'))
    #      #=> f42bb0eeb018ebbd4597ae7213711ec60760843f
    #      instance
    #      #=> f42bb0eeb018ebbd4597ae7213711ec60760843f
    #      instance == other_instance
    #      #=> false
    #      instance.to_s == other_instance.to_s
    #      #=> true
    def initialize(key, digest) end

    # Returns the authentication code an instance represents as a binary string.
    #
    # === Example
    #  instance = OpenSSL::HMAC.new('key', OpenSSL::Digest.new('sha1'))
    #  #=> f42bb0eeb018ebbd4597ae7213711ec60760843f
    #  instance.digest
    #  #=> "\xF4+\xB0\xEE\xB0\x18\xEB\xBDE\x97\xAEr\x13q\x1E\xC6\a`\x84?"
    def digest; end

    # Returns the authentication code an instance represents as a hex-encoded
    # string.
    def hexdigest; end
    alias inspect hexdigest
    alias to_s hexdigest

    def initialize_copy(p1) end

    # Returns _hmac_ as it was when it was first initialized, with all processed
    # data cleared from it.
    #
    # === Example
    #
    #      data = "The quick brown fox jumps over the lazy dog"
    #      instance = OpenSSL::HMAC.new('key', OpenSSL::Digest.new('sha1'))
    #      #=> f42bb0eeb018ebbd4597ae7213711ec60760843f
    #
    #      instance.update(data)
    #      #=> de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9
    #      instance.reset
    #      #=> f42bb0eeb018ebbd4597ae7213711ec60760843f
    def reset; end

    # Returns _hmac_ updated with the message to be authenticated.
    # Can be called repeatedly with chunks of the message.
    #
    # === Example
    #
    #      first_chunk = 'The quick brown fox jumps '
    #      second_chunk = 'over the lazy dog'
    #
    #      instance.update(first_chunk)
    #      #=> 5b9a8038a65d571076d97fe783989e52278a492a
    #      instance.update(second_chunk)
    #      #=> de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9
    def update(string) end
    alias << update
  end

  # Document-class: OpenSSL::HMAC
  #
  # OpenSSL::HMAC allows computing Hash-based Message Authentication Code
  # (HMAC). It is a type of message authentication code (MAC) involving a
  # hash function in combination with a key. HMAC can be used to verify the
  # integrity of a message as well as the authenticity.
  #
  # OpenSSL::HMAC has a similar interface to OpenSSL::Digest.
  #
  # === HMAC-SHA256 using one-shot interface
  #
  #   key = "key"
  #   data = "message-to-be-authenticated"
  #   mac = OpenSSL::HMAC.hexdigest("SHA256", key, data)
  #   #=> "cddb0db23f469c8bf072b21fd837149bd6ace9ab771cceef14c9e517cc93282e"
  #
  # === HMAC-SHA256 using incremental interface
  #
  #   data1 = File.read("file1")
  #   data2 = File.read("file2")
  #   key = "key"
  #   digest = OpenSSL::Digest::SHA256.new
  #   hmac = OpenSSL::HMAC.new(key, digest)
  #   hmac << data1
  #   hmac << data2
  #   mac = hmac.digest
  class HMACError < OpenSSLError
  end

  # Provides functionality of various KDFs (key derivation function).
  #
  # KDF is typically used for securely deriving arbitrary length symmetric
  # keys to be used with an OpenSSL::Cipher from passwords. Another use case
  # is for storing passwords: Due to the ability to tweak the effort of
  # computation by increasing the iteration count, computation can be slowed
  # down artificially in order to render possible attacks infeasible.
  #
  # Currently, OpenSSL::KDF provides implementations for the following KDF:
  #
  # * PKCS #5 PBKDF2 (Password-Based Key Derivation Function 2) in
  #   combination with HMAC
  # * scrypt
  # * HKDF
  #
  # == Examples
  # === Generating a 128 bit key for a Cipher (e.g. AES)
  #   pass = "secret"
  #   salt = OpenSSL::Random.random_bytes(16)
  #   iter = 20_000
  #   key_len = 16
  #   key = OpenSSL::KDF.pbkdf2_hmac(pass, salt: salt, iterations: iter,
  #                                  length: key_len, hash: "sha1")
  #
  # === Storing Passwords
  #   pass = "secret"
  #   # store this with the generated value
  #   salt = OpenSSL::Random.random_bytes(16)
  #   iter = 20_000
  #   hash = OpenSSL::Digest::SHA256.new
  #   len = hash.digest_length
  #   # the final value to be stored
  #   value = OpenSSL::KDF.pbkdf2_hmac(pass, salt: salt, iterations: iter,
  #                                    length: len, hash: hash)
  #
  # == Important Note on Checking Passwords
  # When comparing passwords provided by the user with previously stored
  # values, a common mistake made is comparing the two values using "==".
  # Typically, "==" short-circuits on evaluation, and is therefore
  # vulnerable to timing attacks. The proper way is to use a method that
  # always takes the same amount of time when comparing two values, thus
  # not leaking any information to potential attackers. To compare two
  # values, the following could be used:
  #
  #   def eql_time_cmp(a, b)
  #     unless a.length == b.length
  #       return false
  #     end
  #     cmp = b.bytes
  #     result = 0
  #     a.bytes.each_with_index {|c,i|
  #       result |= c ^ cmp[i]
  #     }
  #     result == 0
  #   end
  #
  # Please note that the premature return in case of differing lengths
  # typically does not leak valuable information - when using PBKDF2, the
  # length of the values to be compared is of fixed size.
  module KDF
    # HMAC-based Extract-and-Expand Key Derivation Function (HKDF) as specified in
    # {RFC 5869}[https://tools.ietf.org/html/rfc5869].
    #
    # New in OpenSSL 1.1.0.
    #
    # === Parameters
    # _ikm_::
    #   The input keying material.
    # _salt_::
    #   The salt.
    # _info_::
    #   The context and application specific information.
    # _length_::
    #   The output length in octets. Must be <= <tt>255 * HashLen</tt>, where
    #   HashLen is the length of the hash function output in octets.
    # _hash_::
    #   The hash function.
    def self.hkdf(ikm, salt:, info:, length:, hash:) end

    # PKCS #5 PBKDF2 (Password-Based Key Derivation Function 2) in combination
    # with HMAC. Takes _pass_, _salt_ and _iterations_, and then derives a key
    # of _length_ bytes.
    #
    # For more information about PBKDF2, see RFC 2898 Section 5.2
    # (https://tools.ietf.org/html/rfc2898#section-5.2).
    #
    # === Parameters
    # pass       :: The passphrase.
    # salt       :: The salt. Salts prevent attacks based on dictionaries of common
    #               passwords and attacks based on rainbow tables. It is a public
    #               value that can be safely stored along with the password (e.g.
    #               if the derived value is used for password storage).
    # iterations :: The iteration count. This provides the ability to tune the
    #               algorithm. It is better to use the highest count possible for
    #               the maximum resistance to brute-force attacks.
    # length     :: The desired length of the derived key in octets.
    # hash       :: The hash algorithm used with HMAC for the PRF. May be a String
    #               representing the algorithm name, or an instance of
    #               OpenSSL::Digest.
    def self.pbkdf2_hmac(pass, salt:, iterations:, length:, hash:) end

    # Derives a key from _pass_ using given parameters with the scrypt
    # password-based key derivation function. The result can be used for password
    # storage.
    #
    # scrypt is designed to be memory-hard and more secure against brute-force
    # attacks using custom hardwares than alternative KDFs such as PBKDF2 or
    # bcrypt.
    #
    # The keyword arguments _N_, _r_ and _p_ can be used to tune scrypt. RFC 7914
    # (published on 2016-08, https://tools.ietf.org/html/rfc7914#section-2) states
    # that using values r=8 and p=1 appears to yield good results.
    #
    # See RFC 7914 (https://tools.ietf.org/html/rfc7914) for more information.
    #
    # === Parameters
    # pass   :: Passphrase.
    # salt   :: Salt.
    # N      :: CPU/memory cost parameter. This must be a power of 2.
    # r      :: Block size parameter.
    # p      :: Parallelization parameter.
    # length :: Length in octets of the derived key.
    #
    # === Example
    #   pass = "password"
    #   salt = SecureRandom.random_bytes(16)
    #   dk = OpenSSL::KDF.scrypt(pass, salt: salt, N: 2**14, r: 8, p: 1, length: 32)
    #   p dk #=> "\xDA\xE4\xE2...\x7F\xA1\x01T"
    def self.scrypt(pass, salt:, n:, r:, p:, length:) end

    private

    # HMAC-based Extract-and-Expand Key Derivation Function (HKDF) as specified in
    # {RFC 5869}[https://tools.ietf.org/html/rfc5869].
    #
    # New in OpenSSL 1.1.0.
    #
    # === Parameters
    # _ikm_::
    #   The input keying material.
    # _salt_::
    #   The salt.
    # _info_::
    #   The context and application specific information.
    # _length_::
    #   The output length in octets. Must be <= <tt>255 * HashLen</tt>, where
    #   HashLen is the length of the hash function output in octets.
    # _hash_::
    #   The hash function.
    def hkdf(ikm, salt:, info:, length:, hash:) end

    # PKCS #5 PBKDF2 (Password-Based Key Derivation Function 2) in combination
    # with HMAC. Takes _pass_, _salt_ and _iterations_, and then derives a key
    # of _length_ bytes.
    #
    # For more information about PBKDF2, see RFC 2898 Section 5.2
    # (https://tools.ietf.org/html/rfc2898#section-5.2).
    #
    # === Parameters
    # pass       :: The passphrase.
    # salt       :: The salt. Salts prevent attacks based on dictionaries of common
    #               passwords and attacks based on rainbow tables. It is a public
    #               value that can be safely stored along with the password (e.g.
    #               if the derived value is used for password storage).
    # iterations :: The iteration count. This provides the ability to tune the
    #               algorithm. It is better to use the highest count possible for
    #               the maximum resistance to brute-force attacks.
    # length     :: The desired length of the derived key in octets.
    # hash       :: The hash algorithm used with HMAC for the PRF. May be a String
    #               representing the algorithm name, or an instance of
    #               OpenSSL::Digest.
    def pbkdf2_hmac(pass, salt:, iterations:, length:, hash:) end

    # Derives a key from _pass_ using given parameters with the scrypt
    # password-based key derivation function. The result can be used for password
    # storage.
    #
    # scrypt is designed to be memory-hard and more secure against brute-force
    # attacks using custom hardwares than alternative KDFs such as PBKDF2 or
    # bcrypt.
    #
    # The keyword arguments _N_, _r_ and _p_ can be used to tune scrypt. RFC 7914
    # (published on 2016-08, https://tools.ietf.org/html/rfc7914#section-2) states
    # that using values r=8 and p=1 appears to yield good results.
    #
    # See RFC 7914 (https://tools.ietf.org/html/rfc7914) for more information.
    #
    # === Parameters
    # pass   :: Passphrase.
    # salt   :: Salt.
    # N      :: CPU/memory cost parameter. This must be a power of 2.
    # r      :: Block size parameter.
    # p      :: Parallelization parameter.
    # length :: Length in octets of the derived key.
    #
    # === Example
    #   pass = "password"
    #   salt = SecureRandom.random_bytes(16)
    #   dk = OpenSSL::KDF.scrypt(pass, salt: salt, N: 2**14, r: 8, p: 1, length: 32)
    #   p dk #=> "\xDA\xE4\xE2...\x7F\xA1\x01T"
    def scrypt(pass, salt:, n:, r:, p:, length:) end

    # Generic exception class raised if an error occurs in OpenSSL::KDF module.
    class KDFError < OpenSSLError
    end
  end

  # OpenSSL::Netscape is a namespace for SPKI (Simple Public Key
  # Infrastructure) which implements Signed Public Key and Challenge.
  # See {RFC 2692}[http://tools.ietf.org/html/rfc2692] and {RFC
  # 2693}[http://tools.ietf.org/html/rfc2692] for details.
  module Netscape
    # A Simple Public Key Infrastructure implementation (pronounced "spooky").
    # The structure is defined as
    #   PublicKeyAndChallenge ::= SEQUENCE {
    #     spki SubjectPublicKeyInfo,
    #     challenge IA5STRING
    #   }
    #
    #   SignedPublicKeyAndChallenge ::= SEQUENCE {
    #     publicKeyAndChallenge PublicKeyAndChallenge,
    #     signatureAlgorithm AlgorithmIdentifier,
    #     signature BIT STRING
    #   }
    # where the definitions of SubjectPublicKeyInfo and AlgorithmIdentifier can
    # be found in RFC5280. SPKI is typically used in browsers for generating
    # a public/private key pair and a subsequent certificate request, using
    # the HTML <keygen> element.
    #
    # == Examples
    #
    # === Creating an SPKI
    #   key = OpenSSL::PKey::RSA.new 2048
    #   spki = OpenSSL::Netscape::SPKI.new
    #   spki.challenge = "RandomChallenge"
    #   spki.public_key = key.public_key
    #   spki.sign(key, OpenSSL::Digest::SHA256.new)
    #   #send a request containing this to a server generating a certificate
    # === Verifying an SPKI request
    #   request = #...
    #   spki = OpenSSL::Netscape::SPKI.new request
    #   unless spki.verify(spki.public_key)
    #     # signature is invalid
    #   end
    #   #proceed
    class SPKI
      # === Parameters
      # * _request_ - optional raw request, either in PEM or DER format.
      def initialize(*request) end

      # Returns the challenge string associated with this SPKI.
      def challenge; end

      # === Parameters
      # * _str_ - the challenge string to be set for this instance
      #
      # Sets the challenge to be associated with the SPKI. May be used by the
      # server, e.g. to prevent replay.
      def challenge=(str) end

      # Returns the public key associated with the SPKI, an instance of
      # OpenSSL::PKey.
      def public_key; end

      # === Parameters
      # * _pub_ - the public key to be set for this instance
      #
      # Sets the public key to be associated with the SPKI, an instance of
      # OpenSSL::PKey. This should be the public key corresponding to the
      # private key used for signing the SPKI.
      def public_key=(pub) end

      # === Parameters
      # * _key_ - the private key to be used for signing this instance
      # * _digest_ - the digest to be used for signing this instance
      #
      # To sign an SPKI, the private key corresponding to the public key set
      # for this instance should be used, in addition to a digest algorithm in
      # the form of an OpenSSL::Digest. The private key should be an instance of
      # OpenSSL::PKey.
      def sign(key, digest) end

      # Returns the DER encoding of this SPKI.
      def to_der; end

      # Returns the PEM encoding of this SPKI.
      def to_pem; end
      alias to_s to_pem

      # Returns a textual representation of this SPKI, useful for debugging
      # purposes.
      def to_text; end

      # === Parameters
      # * _key_ - the public key to be used for verifying the SPKI signature
      #
      # Returns +true+ if the signature is valid, +false+ otherwise. To verify an
      # SPKI, the public key contained within the SPKI should be used.
      def verify(key) end
    end

    # Generic Exception class that is raised if an error occurs during an
    # operation on an instance of OpenSSL::Netscape::SPKI.
    class SPKIError < OpenSSLError
    end
  end

  # OpenSSL::OCSP implements Online Certificate Status Protocol requests
  # and responses.
  #
  # Creating and sending an OCSP request requires a subject certificate
  # that contains an OCSP URL in an authorityInfoAccess extension and the
  # issuer certificate for the subject certificate.  First, load the issuer
  # and subject certificates:
  #
  #   subject = OpenSSL::X509::Certificate.new subject_pem
  #   issuer  = OpenSSL::X509::Certificate.new issuer_pem
  #
  # To create the request we need to create a certificate ID for the
  # subject certificate so the CA knows which certificate we are asking
  # about:
  #
  #   digest = OpenSSL::Digest::SHA1.new
  #   certificate_id =
  #     OpenSSL::OCSP::CertificateId.new subject, issuer, digest
  #
  # Then create a request and add the certificate ID to it:
  #
  #   request = OpenSSL::OCSP::Request.new
  #   request.add_certid certificate_id
  #
  # Adding a nonce to the request protects against replay attacks but not
  # all CA process the nonce.
  #
  #   request.add_nonce
  #
  # To submit the request to the CA for verification we need to extract the
  # OCSP URI from the subject certificate:
  #
  #   authority_info_access = subject.extensions.find do |extension|
  #     extension.oid == 'authorityInfoAccess'
  #   end
  #
  #   descriptions = authority_info_access.value.split "\n"
  #   ocsp = descriptions.find do |description|
  #     description.start_with? 'OCSP'
  #   end
  #
  #   require 'uri'
  #
  #   ocsp_uri = URI ocsp[/URI:(.*)/, 1]
  #
  # To submit the request we'll POST the request to the OCSP URI (per RFC
  # 2560).  Note that we only handle HTTP requests and don't handle any
  # redirects in this example, so this is insufficient for serious use.
  #
  #   require 'net/http'
  #
  #   http_response =
  #     Net::HTTP.start ocsp_uri.hostname, ocsp.port do |http|
  #       http.post ocsp_uri.path, request.to_der,
  #                 'content-type' => 'application/ocsp-request'
  #   end
  #
  #   response = OpenSSL::OCSP::Response.new http_response.body
  #   response_basic = response.basic
  #
  # First we check if the response has a valid signature.  Without a valid
  # signature we cannot trust it.  If you get a failure here you may be
  # missing a system certificate store or may be missing the intermediate
  # certificates.
  #
  #   store = OpenSSL::X509::Store.new
  #   store.set_default_paths
  #
  #   unless response_basic.verify [], store then
  #     raise 'response is not signed by a trusted certificate'
  #   end
  #
  # The response contains the status information (success/fail).  We can
  # display the status as a string:
  #
  #   puts response.status_string #=> successful
  #
  # Next we need to know the response details to determine if the response
  # matches our request.  First we check the nonce.  Again, not all CAs
  # support a nonce.  See Request#check_nonce for the meanings of the
  # return values.
  #
  #   p request.check_nonce basic_response #=> value from -1 to 3
  #
  # Then extract the status information for the certificate from the basic
  # response.
  #
  #   single_response = basic_response.find_response(certificate_id)
  #
  #   unless single_response
  #     raise 'basic_response does not have the status for the certificiate'
  #   end
  #
  # Then check the validity. A status issued in the future must be rejected.
  #
  #   unless single_response.check_validity
  #     raise 'this_update is in the future or next_update time has passed'
  #   end
  #
  #   case single_response.cert_status
  #   when OpenSSL::OCSP::V_CERTSTATUS_GOOD
  #     puts 'certificate is still valid'
  #   when OpenSSL::OCSP::V_CERTSTATUS_REVOKED
  #     puts "certificate has been revoked at #{single_response.revocation_time}"
  #   when OpenSSL::OCSP::V_CERTSTATUS_UNKNOWN
  #     puts 'responder doesn't know about the certificate'
  #   end
  module OCSP
    # (This flag is not used by OpenSSL 1.0.1g)
    NOCASIGN = _
    # Do not include certificates in the response
    NOCERTS = _
    # Do not verify the certificate chain on the response
    NOCHAIN = _
    # Do not make additional signing certificate checks
    NOCHECKS = _
    # (This flag is not used by OpenSSL 1.0.1g)
    NODELEGATED = _
    # Do not check trust
    NOEXPLICIT = _
    # Do not search certificates contained in the response for a signer
    NOINTERN = _
    # Do not check the signature on the response
    NOSIGS = _
    # Do not include producedAt time in response
    NOTIME = _
    # Do not verify the response at all
    NOVERIFY = _
    # Identify the response by signing the certificate key ID
    RESPID_KEY = _
    # Internal error in issuer
    RESPONSE_STATUS_INTERNALERROR = _
    # Illegal confirmation request
    RESPONSE_STATUS_MALFORMEDREQUEST = _
    # You must sign the request and resubmit
    RESPONSE_STATUS_SIGREQUIRED = _
    # Response has valid confirmations
    RESPONSE_STATUS_SUCCESSFUL = _
    # Try again later
    RESPONSE_STATUS_TRYLATER = _
    # Your request is unauthorized.
    RESPONSE_STATUS_UNAUTHORIZED = _
    # The certificate subject's name or other information changed
    REVOKED_STATUS_AFFILIATIONCHANGED = _
    # This CA certificate was revoked due to a key compromise
    REVOKED_STATUS_CACOMPROMISE = _
    # The certificate is on hold
    REVOKED_STATUS_CERTIFICATEHOLD = _
    # The certificate is no longer needed
    REVOKED_STATUS_CESSATIONOFOPERATION = _
    # The certificate was revoked due to a key compromise
    REVOKED_STATUS_KEYCOMPROMISE = _
    # The certificate was revoked for an unknown reason
    REVOKED_STATUS_NOSTATUS = _
    # The certificate was previously on hold and should now be removed from
    # the CRL
    REVOKED_STATUS_REMOVEFROMCRL = _
    # The certificate was superseded by a new certificate
    REVOKED_STATUS_SUPERSEDED = _
    # The certificate was revoked for an unspecified reason
    REVOKED_STATUS_UNSPECIFIED = _
    # Do not verify additional certificates
    TRUSTOTHER = _
    # Indicates the certificate is not revoked but does not necessarily mean
    # the certificate was issued or that this response is within the
    # certificate's validity interval
    V_CERTSTATUS_GOOD = _
    # Indicates the certificate has been revoked either permanently or
    # temporarily (on hold).
    V_CERTSTATUS_REVOKED = _
    # Indicates the responder does not know about the certificate being
    # requested.
    V_CERTSTATUS_UNKNOWN = _
    # The responder ID is based on the public key.
    V_RESPID_KEY = _
    # The responder ID is based on the key name.
    V_RESPID_NAME = _

    # An OpenSSL::OCSP::BasicResponse contains the status of a certificate
    # check which is created from an OpenSSL::OCSP::Request.  A
    # BasicResponse is more detailed than a Response.
    class BasicResponse
      # Creates a new BasicResponse. If _der_string_ is given, decodes _der_string_
      # as DER.
      def initialize(der_string = nil) end

      # Adds _nonce_ to this response.  If no nonce was provided a random nonce
      # will be added.
      def add_nonce(nonce = nil) end

      # Adds a certificate status for _certificate_id_. _status_ is the status, and
      # must be one of these:
      #
      # - OpenSSL::OCSP::V_CERTSTATUS_GOOD
      # - OpenSSL::OCSP::V_CERTSTATUS_REVOKED
      # - OpenSSL::OCSP::V_CERTSTATUS_UNKNOWN
      #
      # _reason_ and _revocation_time_ can be given only when _status_ is
      # OpenSSL::OCSP::V_CERTSTATUS_REVOKED. _reason_ describes the reason for the
      # revocation, and must be one of OpenSSL::OCSP::REVOKED_STATUS_* constants.
      # _revocation_time_ is the time when the certificate is revoked.
      #
      # _this_update_ and _next_update_ indicate the time at which ths status is
      # verified to be correct and the time at or before which newer information
      # will be available, respectively. _next_update_ is optional.
      #
      # _extensions_ is an Array of OpenSSL::X509::Extension to be included in the
      # SingleResponse. This is also optional.
      #
      # Note that the times, _revocation_time_, _this_update_ and _next_update_
      # can be specified in either of Integer or Time object. If they are Integer, it
      # is treated as the relative seconds from the current time.
      def add_status(certificate_id, status, reason, revocation_time, this_update, next_update, extensions) end

      # Copies the nonce from _request_ into this response.  Returns 1 on success
      # and 0 on failure.
      def copy_nonce(request) end

      # Returns a SingleResponse whose CertId matches with _certificate_id_, or +nil+
      # if this BasicResponse does not contain it.
      def find_response(certificate_id) end

      def initialize_copy(p1) end

      # Returns an Array of SingleResponse for this BasicResponse.
      def responses; end

      # Signs this OCSP response using the _cert_, _key_ and optional _digest_. This
      # behaves in the similar way as OpenSSL::OCSP::Request#sign.
      #
      # _flags_ can include:
      # OpenSSL::OCSP::NOCERTS::    don't include certificates
      # OpenSSL::OCSP::NOTIME::     don't set producedAt
      # OpenSSL::OCSP::RESPID_KEY:: use signer's public key hash as responderID
      def sign(cert, key, certs = nil, flags = 0, digest = nil) end

      # Returns an Array of statuses for this response.  Each status contains a
      # CertificateId, the status (0 for good, 1 for revoked, 2 for unknown), the
      # reason for the status, the revocation time, the time of this update, the time
      # for the next update and a list of OpenSSL::X509::Extension.
      #
      # This should be superseded by BasicResponse#responses and #find_response that
      # return SingleResponse.
      def status; end

      # Encodes this basic response into a DER-encoded string.
      def to_der; end

      # Verifies the signature of the response using the given _certificates_ and
      # _store_. This works in the similar way as OpenSSL::OCSP::Request#verify.
      def verify(certificates, store, flags = 0) end
    end

    # An OpenSSL::OCSP::CertificateId identifies a certificate to the CA so
    # that a status check can be performed.
    class CertificateId
      # Creates a new OpenSSL::OCSP::CertificateId for the given _subject_ and
      # _issuer_ X509 certificates.  The _digest_ is a digest algorithm that is used
      # to compute the hash values. This defaults to SHA-1.
      #
      # If only one argument is given, decodes it as DER representation of a
      # certificate ID.
      def initialize(*several_variants) end

      # Compares this certificate id with _other_ and returns +true+ if they are the
      # same.
      def cmp(other) end

      # Compares this certificate id's issuer with _other_ and returns +true+ if
      # they are the same.
      def cmp_issuer(other) end

      # Returns the ln (long name) of the hash algorithm used to generate
      # the issuerNameHash and the issuerKeyHash values.
      def hash_algorithm; end

      def initialize_copy(p1) end

      # Returns the issuerKeyHash of this certificate ID, the hash of the issuer's
      # public key.
      def issuer_key_hash; end

      # Returns the issuerNameHash of this certificate ID, the hash of the
      # issuer's distinguished name calculated with the hashAlgorithm.
      def issuer_name_hash; end

      # Returns the serial number of the certificate for which status is being
      # requested.
      def serial; end

      # Encodes this certificate identifier into a DER-encoded string.
      def to_der; end
    end

    # OCSP error class.
    class OCSPError < OpenSSLError
    end

    # An OpenSSL::OCSP::Request contains the certificate information for
    # determining if a certificate has been revoked or not.  A Request can be
    # created for a certificate or from a DER-encoded request created
    # elsewhere.
    class Request
      # Creates a new OpenSSL::OCSP::Request.  The request may be created empty or
      # from a _request_der_ string.
      def initialize(*several_variants) end

      # Adds _certificate_id_ to the request.
      def add_certid(certificate_id) end

      # Adds a _nonce_ to the OCSP request.  If no nonce is given a random one will
      # be generated.
      #
      # The nonce is used to prevent replay attacks but some servers do not support
      # it.
      def add_nonce(nonce = nil) end

      # Returns all certificate IDs in this request.
      def certid; end

      # Checks the nonce validity for this request and _response_.
      #
      # The return value is one of the following:
      #
      # -1 :: nonce in request only.
      #  0 :: nonces both present and not equal.
      #  1 :: nonces present and equal.
      #  2 :: nonces both absent.
      #  3 :: nonce present in response only.
      #
      # For most responses, clients can check _result_ > 0.  If a responder doesn't
      # handle nonces <code>result.nonzero?</code> may be necessary.  A result of
      # <code>0</code> is always an error.
      def check_nonce(response) end

      def initialize_copy(p1) end

      # Signs this OCSP request using _cert_, _key_ and optional _digest_. If
      # _digest_ is not specified, SHA-1 is used. _certs_ is an optional Array of
      # additional certificates which are included in the request in addition to
      # the signer certificate. Note that if _certs_ is +nil+ or not given, flag
      # OpenSSL::OCSP::NOCERTS is enabled. Pass an empty array to include only the
      # signer certificate.
      #
      # _flags_ is a bitwise OR of the following constants:
      #
      # OpenSSL::OCSP::NOCERTS::
      #   Don't include any certificates in the request. _certs_ will be ignored.
      def sign(cert, key, certs = nil, flags = 0, digest = nil) end

      # Returns +true+ if the request is signed, +false+ otherwise. Note that the
      # validity of the signature is *not* checked. Use #verify to verify that.
      def signed?; end

      # Returns this request as a DER-encoded string
      def to_der; end

      # Verifies this request using the given _certificates_ and _store_.
      # _certificates_ is an array of OpenSSL::X509::Certificate, _store_ is an
      # OpenSSL::X509::Store.
      #
      # Note that +false+ is returned if the request does not have a signature.
      # Use #signed? to check whether the request is signed or not.
      def verify(certificates, store, flags = 0) end
    end

    # An OpenSSL::OCSP::Response contains the status of a certificate check
    # which is created from an OpenSSL::OCSP::Request.
    class Response
      # Creates an OpenSSL::OCSP::Response from _status_ and _basic_response_.
      def self.create(status, basic_response = nil) end

      # Creates a new OpenSSL::OCSP::Response.  The response may be created empty or
      # from a _response_der_ string.
      def initialize(*several_variants) end

      # Returns a BasicResponse for this response
      def basic; end

      def initialize_copy(p1) end

      # Returns the status of the response.
      def status; end

      # Returns a status string for the response.
      def status_string; end

      # Returns this response as a DER-encoded string.
      def to_der; end
    end

    # An OpenSSL::OCSP::SingleResponse represents an OCSP SingleResponse
    # structure, which contains the basic information of the status of the
    # certificate.
    class SingleResponse
      # Creates a new SingleResponse from _der_string_.
      def initialize(der_string) end

      # Returns the status of the certificate identified by the certid.
      # The return value may be one of these constant:
      #
      # - V_CERTSTATUS_GOOD
      # - V_CERTSTATUS_REVOKED
      # - V_CERTSTATUS_UNKNOWN
      #
      # When the status is V_CERTSTATUS_REVOKED, the time at which the certificate
      # was revoked can be retrieved by #revocation_time.
      def cert_status; end

      # Returns the CertificateId for which this SingleResponse is.
      def certid; end

      # Checks the validity of thisUpdate and nextUpdate fields of this
      # SingleResponse. This checks the current time is within the range thisUpdate
      # to nextUpdate.
      #
      # It is possible that the OCSP request takes a few seconds or the time is not
      # accurate. To avoid rejecting a valid response, this method allows the times
      # to be within _nsec_ seconds of the current time.
      #
      # Some responders don't set the nextUpdate field. This may cause a very old
      # response to be considered valid. The _maxsec_ parameter can be used to limit
      # the age of responses.
      def check_validity(nsec = 0, maxsec = -1) end

      def extensions; end

      def initialize_copy(p1) end

      def next_update; end

      def revocation_reason; end

      def revocation_time; end

      def this_update; end

      # Encodes this SingleResponse into a DER-encoded string.
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
    # * _pass_ - string
    # * _name_ - A string describing the key.
    # * _key_ - Any PKey.
    # * _cert_ - A X509::Certificate.
    #   * The public_key portion of the certificate must contain a valid public key.
    #   * The not_before and not_after fields must be filled in.
    # * _ca_ - An optional array of X509::Certificate's.
    # * _key_pbe_ - string
    # * _cert_pbe_ - string
    # * _key_iter_ - integer
    # * _mac_iter_ - integer
    # * _keytype_ - An integer representing an MSIE specific extension.
    #
    # Any optional arguments may be supplied as +nil+ to preserve the OpenSSL defaults.
    #
    # See the OpenSSL documentation for PKCS12_create().
    def self.create(p1, p2, p3, p4, p5 = v5, p6 = v6, p7 = v7, p8 = v8, p9 = v9, p10 = v10) end

    # === Parameters
    # * _str_ - Must be a DER encoded PKCS12 string.
    # * _pass_ - string
    def initialize(*several_variants) end

    def initialize_copy(p1) end

    def to_der; end

    class PKCS12Error < OpenSSLError
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

    def decrypt(p1, p2 = v2, p3 = v3) end

    def detached; end

    def detached=(p1) end

    def detached?; end

    def initialize_copy(p1) end

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

  # == Asymmetric Public Key Algorithms
  #
  # Asymmetric public key algorithms solve the problem of establishing and
  # sharing secret keys to en-/decrypt messages. The key in such an
  # algorithm consists of two parts: a public key that may be distributed
  # to others and a private key that needs to remain secret.
  #
  # Messages encrypted with a public key can only be decrypted by
  # recipients that are in possession of the associated private key.
  # Since public key algorithms are considerably slower than symmetric
  # key algorithms (cf. OpenSSL::Cipher) they are often used to establish
  # a symmetric key shared between two parties that are in possession of
  # each other's public key.
  #
  # Asymmetric algorithms offer a lot of nice features that are used in a
  # lot of different areas. A very common application is the creation and
  # validation of digital signatures. To sign a document, the signatory
  # generally uses a message digest algorithm (cf. OpenSSL::Digest) to
  # compute a digest of the document that is then encrypted (i.e. signed)
  # using the private key. Anyone in possession of the public key may then
  # verify the signature by computing the message digest of the original
  # document on their own, decrypting the signature using the signatory's
  # public key and comparing the result to the message digest they
  # previously computed. The signature is valid if and only if the
  # decrypted signature is equal to this message digest.
  #
  # The PKey module offers support for three popular public/private key
  # algorithms:
  # * RSA (OpenSSL::PKey::RSA)
  # * DSA (OpenSSL::PKey::DSA)
  # * Elliptic Curve Cryptography (OpenSSL::PKey::EC)
  # Each of these implementations is in fact a sub-class of the abstract
  # PKey class which offers the interface for supporting digital signatures
  # in the form of PKey#sign and PKey#verify.
  #
  # == Diffie-Hellman Key Exchange
  #
  # Finally PKey also features OpenSSL::PKey::DH, an implementation of
  # the Diffie-Hellman key exchange protocol based on discrete logarithms
  # in finite fields, the same basis that DSA is built on.
  # The Diffie-Hellman protocol can be used to exchange (symmetric) keys
  # over insecure channels without needing any prior joint knowledge
  # between the participating parties. As the security of DH demands
  # relatively long "public keys" (i.e. the part that is overtly
  # transmitted between participants) DH tends to be quite slow. If
  # security or speed is your primary concern, OpenSSL::PKey::EC offers
  # another implementation of the Diffie-Hellman protocol.
  module PKey
    # Reads a DER or PEM encoded string from _string_ or _io_ and returns an
    # instance of the appropriate PKey class.
    #
    # === Parameters
    # * _string+ is a DER- or PEM-encoded string containing an arbitrary private
    #   or public key.
    # * _io_ is an instance of IO containing a DER- or PEM-encoded
    #   arbitrary private or public key.
    # * _pwd_ is an optional password in case _string_ or _io_ is an encrypted
    #   PEM resource.
    def self.read(*several_variants) end

    private

    # Reads a DER or PEM encoded string from _string_ or _io_ and returns an
    # instance of the appropriate PKey class.
    #
    # === Parameters
    # * _string+ is a DER- or PEM-encoded string containing an arbitrary private
    #   or public key.
    # * _io_ is an instance of IO containing a DER- or PEM-encoded
    #   arbitrary private or public key.
    # * _pwd_ is an optional password in case _string_ or _io_ is an encrypted
    #   PEM resource.
    def read(*several_variants) end

    # An implementation of the Diffie-Hellman key exchange protocol based on
    # discrete logarithms in finite fields, the same basis that DSA is built
    # on.
    #
    # === Accessor methods for the Diffie-Hellman parameters
    # DH#p::
    #   The prime (an OpenSSL::BN) of the Diffie-Hellman parameters.
    # DH#g::
    #   The generator (an OpenSSL::BN) g of the Diffie-Hellman parameters.
    # DH#pub_key::
    #   The per-session public key (an OpenSSL::BN) matching the private key.
    #   This needs to be passed to DH#compute_key.
    # DH#priv_key::
    #   The per-session private key, an OpenSSL::BN.
    #
    # === Example of a key exchange
    #  dh1 = OpenSSL::PKey::DH.new(2048)
    #  der = dh1.public_key.to_der #you may send this publicly to the participating party
    #  dh2 = OpenSSL::PKey::DH.new(der)
    #  dh2.generate_key! #generate the per-session key pair
    #  symm_key1 = dh1.compute_key(dh2.pub_key)
    #  symm_key2 = dh2.compute_key(dh1.pub_key)
    #
    #  puts symm_key1 == symm_key2 # => true
    class DH < PKey
      # Creates a new DH instance from scratch by generating the private and public
      # components alike.
      #
      # === Parameters
      # * _size_ is an integer representing the desired key size. Keys smaller than 1024 bits should be considered insecure.
      # * _generator_ is a small number > 1, typically 2 or 5.
      def self.generate(p1, p2 = v2) end

      # Either generates a DH instance from scratch or by reading already existing
      # DH parameters from _string_. Note that when reading a DH instance from
      # data that was encoded from a DH instance by using DH#to_pem or DH#to_der
      # the result will *not* contain a public/private key pair yet. This needs to
      # be generated using DH#generate_key! first.
      #
      # === Parameters
      # * _size_ is an integer representing the desired key size. Keys smaller than 1024 bits should be considered insecure.
      # * _generator_ is a small number > 1, typically 2 or 5.
      # * _string_ contains the DER or PEM encoded key.
      #
      # === Examples
      #  DH.new # -> dh
      #  DH.new(1024) # -> dh
      #  DH.new(1024, 5) # -> dh
      #  #Reading DH parameters
      #  dh = DH.new(File.read('parameters.pem')) # -> dh, but no public/private key yet
      #  dh.generate_key! # -> dh with public and private key
      def initialize(*several_variants) end

      # Returns a String containing a shared secret computed from the other party's public value.
      # See DH_compute_key() for further information.
      #
      # === Parameters
      # * _pub_bn_ is a OpenSSL::BN, *not* the DH instance returned by
      #   DH#public_key as that contains the DH parameters only.
      def compute_key(pub_bn) end

      # Encodes this DH to its PEM encoding. Note that any existing per-session
      # public/private keys will *not* get encoded, just the Diffie-Hellman
      # parameters will be encoded.
      def export; end
      alias to_pem export
      alias to_s export

      # Generates a private and public key unless a private key already exists.
      # If this DH instance was generated from public DH parameters (e.g. by
      # encoding the result of DH#public_key), then this method needs to be
      # called first in order to generate the per-session keys before performing
      # the actual key exchange.
      #
      # === Example
      #   dh = OpenSSL::PKey::DH.new(2048)
      #   public_key = dh.public_key #contains no private/public key yet
      #   public_key.generate_key!
      #   puts public_key.private? # => true
      def generate_key!; end

      def initialize_copy(p1) end

      # Stores all parameters of key to the hash
      # INSECURE: PRIVATE INFORMATIONS CAN LEAK OUT!!!
      # Don't use :-)) (I's up to you)
      def params; end

      # Validates the Diffie-Hellman parameters associated with this instance.
      # It checks whether a safe prime and a suitable generator are used. If this
      # is not the case, +false+ is returned.
      def params_ok?; end

      # Indicates whether this DH instance has a private key associated with it or
      # not. The private key may be retrieved with DH#priv_key.
      def private?; end

      # Indicates whether this DH instance has a public key associated with it or
      # not. The public key may be retrieved with DH#pub_key.
      def public?; end

      # Returns a new DH instance that carries just the public information, i.e.
      # the prime _p_ and the generator _g_, but no public/private key yet. Such
      # a pair may be generated using DH#generate_key!. The "public key" needed
      # for a key exchange with DH#compute_key is considered as per-session
      # information and may be retrieved with DH#pub_key once a key pair has
      # been generated.
      # If the current instance already contains private information (and thus a
      # valid public/private key pair), this information will no longer be present
      # in the new instance generated by DH#public_key. This feature is helpful for
      # publishing the Diffie-Hellman parameters without leaking any of the private
      # per-session information.
      #
      # === Example
      #  dh = OpenSSL::PKey::DH.new(2048) # has public and private key set
      #  public_key = dh.public_key # contains only prime and generator
      #  parameters = public_key.to_der # it's safe to publish this
      def public_key; end

      # Sets _pub_key_ and _priv_key_ for the DH instance. _priv_key_ may be +nil+.
      def set_key(pub_key, priv_key) end

      # Sets _p_, _q_, _g_ to the DH instance.
      def set_pqg(p, q, g) end

      # Encodes this DH to its DER encoding. Note that any existing per-session
      # public/private keys will *not* get encoded, just the Diffie-Hellman
      # parameters will be encoded.
      def to_der; end

      # Prints all parameters of key to buffer
      # INSECURE: PRIVATE INFORMATIONS CAN LEAK OUT!!!
      # Don't use :-)) (I's up to you)
      def to_text; end
    end

    # Generic exception that is raised if an operation on a DH PKey
    # fails unexpectedly or in case an instantiation of an instance of DH
    # fails due to non-conformant input data.
    class DHError < PKeyError
    end

    # DSA, the Digital Signature Algorithm, is specified in NIST's
    # FIPS 186-3. It is an asymmetric public key algorithm that may be used
    # similar to e.g. RSA.
    class DSA < PKey
      # Creates a new DSA instance by generating a private/public key pair
      # from scratch.
      #
      # === Parameters
      # * _size_ is an integer representing the desired key size.
      def self.generate(size) end

      # Creates a new DSA instance by reading an existing key from _string_.
      #
      # === Parameters
      # * _size_ is an integer representing the desired key size.
      # * _string_ contains a DER or PEM encoded key.
      # * _pass_ is a string that contains an optional password.
      #
      # === Examples
      #  DSA.new -> dsa
      #  DSA.new(1024) -> dsa
      #  DSA.new(File.read('dsa.pem')) -> dsa
      #  DSA.new(File.read('dsa.pem'), 'mypassword') -> dsa
      def initialize(*several_variants) end

      # Encodes this DSA to its PEM encoding.
      #
      # === Parameters
      # * _cipher_ is an OpenSSL::Cipher.
      # * _password_ is a string containing your password.
      #
      # === Examples
      #  DSA.to_pem -> aString
      #  DSA.to_pem(cipher, 'mypassword') -> aString
      def export(*args) end
      alias to_pem export
      alias to_s export

      def initialize_copy(p1) end

      # Stores all parameters of key to the hash
      # INSECURE: PRIVATE INFORMATIONS CAN LEAK OUT!!!
      # Don't use :-)) (I's up to you)
      def params; end

      # Indicates whether this DSA instance has a private key associated with it or
      # not. The private key may be retrieved with DSA#private_key.
      def private?; end

      # Indicates whether this DSA instance has a public key associated with it or
      # not. The public key may be retrieved with DSA#public_key.
      def public?; end

      # Returns a new DSA instance that carries just the public key information.
      # If the current instance has also private key information, this will no
      # longer be present in the new instance. This feature is helpful for
      # publishing the public key information without leaking any of the private
      # information.
      #
      # === Example
      #  dsa = OpenSSL::PKey::DSA.new(2048) # has public and private information
      #  pub_key = dsa.public_key # has only the public part available
      #  pub_key_der = pub_key.to_der # it's safe to publish this
      def public_key; end

      # Sets _pub_key_ and _priv_key_ for the DSA instance. _priv_key_ may be +nil+.
      def set_key(pub_key, priv_key) end

      # Sets _p_, _q_, _g_ to the DSA instance.
      def set_pqg(p, q, g) end

      # Computes and returns the DSA signature of _string_, where _string_ is
      # expected to be an already-computed message digest of the original input
      # data. The signature is issued using the private key of this DSA instance.
      #
      # === Parameters
      # * _string_ is a message digest of the original input data to be signed.
      #
      # === Example
      #  dsa = OpenSSL::PKey::DSA.new(2048)
      #  doc = "Sign me"
      #  digest = OpenSSL::Digest::SHA1.digest(doc)
      #  sig = dsa.syssign(digest)
      def syssign(string) end

      # Verifies whether the signature is valid given the message digest input. It
      # does so by validating _sig_ using the public key of this DSA instance.
      #
      # === Parameters
      # * _digest_ is a message digest of the original input data to be signed
      # * _sig_ is a DSA signature value
      #
      # === Example
      #  dsa = OpenSSL::PKey::DSA.new(2048)
      #  doc = "Sign me"
      #  digest = OpenSSL::Digest::SHA1.digest(doc)
      #  sig = dsa.syssign(digest)
      #  puts dsa.sysverify(digest, sig) # => true
      def sysverify(digest, sig) end

      # Encodes this DSA to its DER encoding.
      def to_der; end

      # Prints all parameters of key to buffer
      # INSECURE: PRIVATE INFORMATIONS CAN LEAK OUT!!!
      # Don't use :-)) (I's up to you)
      def to_text; end
    end

    # Generic exception that is raised if an operation on a DSA PKey
    # fails unexpectedly or in case an instantiation of an instance of DSA
    # fails due to non-conformant input data.
    class DSAError < PKeyError
    end

    # OpenSSL::PKey::EC provides access to Elliptic Curve Digital Signature
    # Algorithm (ECDSA) and Elliptic Curve Diffie-Hellman (ECDH).
    #
    # === Key exchange
    #   ec1 = OpenSSL::PKey::EC.generate("prime256v1")
    #   ec2 = OpenSSL::PKey::EC.generate("prime256v1")
    #   # ec1 and ec2 have own private key respectively
    #   shared_key1 = ec1.dh_compute_key(ec2.public_key)
    #   shared_key2 = ec2.dh_compute_key(ec1.public_key)
    #
    #   p shared_key1 == shared_key2 #=> true
    class EC < PKey
      EXPLICIT_CURVE = _
      NAMED_CURVE = _

      # Obtains a list of all predefined curves by the OpenSSL. Curve names are
      # returned as sn.
      #
      # See the OpenSSL documentation for EC_get_builtin_curves().
      def self.builtin_curves; end

      # Creates a new EC instance with a new random private and public key.
      def self.generate(*several_variants) end

      # Creates a new EC object from given arguments.
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

      # Outputs the EC key in PEM encoding.  If _cipher_ and _pass_phrase_ are given
      # they will be used to encrypt the key.  _cipher_ must be an OpenSSL::Cipher
      # instance. Note that encryption will only be effective for a private key,
      # public keys will always be encoded in plain text.
      def export(*args) end
      alias to_pem export

      # Generates a new random private and public key.
      #
      # See also the OpenSSL documentation for EC_KEY_generate_key()
      #
      # === Example
      #   ec = OpenSSL::PKey::EC.new("prime256v1")
      #   p ec.private_key # => nil
      #   ec.generate_key!
      #   p ec.private_key # => #<OpenSSL::BN XXXXXX>
      def generate_key!; end
      alias generate_key generate_key!

      # Returns the EC::Group that the key is associated with. Modifying the returned
      # group does not affect _key_.
      def group; end

      # Sets the EC::Group for the key. The group structure is internally copied so
      # modification to _group_ after assigning to a key has no effect on the key.
      def group=(group) end

      def initialize_copy(p1) end

      # Returns whether this EC instance has a private key. The private key (BN) can
      # be retrieved with EC#private_key.
      def private?; end
      alias private_key? private?

      # See the OpenSSL documentation for EC_KEY_get0_private_key()
      def private_key; end

      # See the OpenSSL documentation for EC_KEY_set_private_key()
      def private_key=(openssl_bn) end

      # Returns whether this EC instance has a public key. The public key
      # (EC::Point) can be retrieved with EC#public_key.
      def public?; end
      alias public_key? public?

      # See the OpenSSL documentation for EC_KEY_get0_public_key()
      def public_key; end

      # See the OpenSSL documentation for EC_KEY_set_public_key()
      def public_key=(ec_point) end

      # See the OpenSSL documentation for i2d_ECPrivateKey_bio()
      def to_der; end

      # See the OpenSSL documentation for EC_KEY_print()
      def to_text; end

      class Group
        # Creates a new EC::Group object.
        #
        # _ec_method_ is a symbol that represents an EC_METHOD. Currently the following
        # are supported:
        #
        # * :GFp_simple
        # * :GFp_mont
        # * :GFp_nist
        # * :GF2m_simple
        #
        # If the first argument is :GFp or :GF2m, creates a new curve with given
        # parameters.
        def initialize(*several_variants) end

        # Returns the flags set on the group.
        #
        # See also #asn1_flag=.
        def asn1_flag; end

        # Sets flags on the group. The flag value is used to determine how to encode
        # the group: encode explicit parameters or named curve using an OID.
        #
        # The flag value can be either of:
        #
        # * EC::NAMED_CURVE
        # * EC::EXPLICIT_CURVE
        #
        # See the OpenSSL documentation for EC_GROUP_set_asn1_flag().
        def asn1_flag=(flags) end

        # Returns the cofactor of the group.
        #
        # See the OpenSSL documentation for EC_GROUP_get_cofactor()
        def cofactor; end

        # Returns the curve name (sn).
        #
        # See the OpenSSL documentation for EC_GROUP_get_curve_name()
        def curve_name; end

        # See the OpenSSL documentation for EC_GROUP_get_degree()
        def degree; end

        # Returns +true+ if the two groups use the same curve and have the same
        # parameters, +false+ otherwise.
        def eql?(other) end
        alias == eql?

        # Returns the generator of the group.
        #
        # See the OpenSSL documentation for EC_GROUP_get0_generator()
        def generator; end

        def initialize_copy(p1) end

        # Returns the order of the group.
        #
        # See the OpenSSL documentation for EC_GROUP_get_order()
        def order; end

        # Returns the form how EC::Point data is encoded as ASN.1.
        #
        # See also #point_conversion_form=.
        def point_conversion_form; end

        # Sets the form how EC::Point data is encoded as ASN.1 as defined in X9.62.
        #
        # _format_ can be one of these:
        #
        # +:compressed+::
        #   Encoded as z||x, where z is an octet indicating which solution of the
        #   equation y is. z will be 0x02 or 0x03.
        # +:uncompressed+::
        #   Encoded as z||x||y, where z is an octet 0x04.
        # +:hybrid+::
        #   Encodes as z||x||y, where z is an octet indicating which solution of the
        #   equation y is. z will be 0x06 or 0x07.
        #
        # See the OpenSSL documentation for EC_GROUP_set_point_conversion_form()
        def point_conversion_form=(form) end

        # See the OpenSSL documentation for EC_GROUP_get0_seed()
        def seed; end

        # See the OpenSSL documentation for EC_GROUP_set_seed()
        def seed=(seed) end

        # Sets the curve parameters. _generator_ must be an instance of EC::Point that
        # is on the curve. _order_ and _cofactor_ are integers.
        #
        # See the OpenSSL documentation for EC_GROUP_set_generator()
        def set_generator(generator, order, cofactor) end

        # See the OpenSSL documentation for i2d_ECPKParameters_bio()
        def to_der; end

        #  See the OpenSSL documentation for PEM_write_bio_ECPKParameters()
        def to_pem; end

        # See the OpenSSL documentation for ECPKParameters_print()
        def to_text; end

        class Error < OpenSSLError
        end
      end

      class Point
        # Creates a new instance of OpenSSL::PKey::EC::Point. If the only argument is
        # an instance of EC::Point, a copy is returned. Otherwise, creates a point
        # that belongs to _group_.
        #
        # _encoded_point_ is the octet string representation of the point. This
        # must be either a String or an OpenSSL::BN.
        def initialize(*several_variants) end

        def eql?(other) end
        alias == eql?

        def infinity?; end

        def initialize_copy(p1) end

        def invert!; end

        def make_affine!; end

        # Performs elliptic curve point multiplication.
        #
        # The first form calculates <tt>bn1 * point + bn2 * G</tt>, where +G+ is the
        # generator of the group of _point_. _bn2_ may be omitted, and in that case,
        # the result is just <tt>bn1 * point</tt>.
        #
        # The second form calculates <tt>bns[0] * point + bns[1] * points[0] + ...
        # + bns[-1] * points[-1] + bn2 * G</tt>. _bn2_ may be omitted. _bns_ must be
        # an array of OpenSSL::BN. _points_ must be an array of
        # OpenSSL::PKey::EC::Point. Please note that <tt>points[0]</tt> is not
        # multiplied by <tt>bns[0]</tt>, but <tt>bns[1]</tt>.
        def mul(*several_variants) end

        def on_curve?; end

        def set_to_infinity!; end

        # Returns the octet string representation of the elliptic curve point.
        #
        # _conversion_form_ specifies how the point is converted. Possible values are:
        #
        # - +:compressed+
        # - +:uncompressed+
        # - +:hybrid+
        def to_octet_string(conversion_form) end

        class Error < OpenSSLError
        end
      end
    end

    class ECError < PKeyError
    end

    # An abstract class that bundles signature creation (PKey#sign) and
    # validation (PKey#verify) that is common to all implementations except
    # OpenSSL::PKey::DH
    # * OpenSSL::PKey::RSA
    # * OpenSSL::PKey::DSA
    # * OpenSSL::PKey::EC
    class PKey
      # Because PKey is an abstract class, actually calling this method explicitly
      # will raise a NotImplementedError.
      def initialize; end

      # To sign the String _data_, _digest_, an instance of OpenSSL::Digest, must
      # be provided. The return value is again a String containing the signature.
      # A PKeyError is raised should errors occur.
      # Any previous state of the Digest instance is irrelevant to the signature
      # outcome, the digest instance is reset to its initial state during the
      # operation.
      #
      # == Example
      #   data = 'Sign me!'
      #   digest = OpenSSL::Digest::SHA256.new
      #   pkey = OpenSSL::PKey::RSA.new(2048)
      #   signature = pkey.sign(digest, data)
      def sign(digest, data) end

      # To verify the String _signature_, _digest_, an instance of
      # OpenSSL::Digest, must be provided to re-compute the message digest of the
      # original _data_, also a String. The return value is +true+ if the
      # signature is valid, +false+ otherwise. A PKeyError is raised should errors
      # occur.
      # Any previous state of the Digest instance is irrelevant to the validation
      # outcome, the digest instance is reset to its initial state during the
      # operation.
      #
      # == Example
      #   data = 'Sign me!'
      #   digest = OpenSSL::Digest::SHA256.new
      #   pkey = OpenSSL::PKey::RSA.new(2048)
      #   signature = pkey.sign(digest, data)
      #   pub_key = pkey.public_key
      #   puts pub_key.verify(digest, signature, data) # => true
      def verify(digest, signature, data) end
    end

    # Raised when errors occur during PKey#sign or PKey#verify.
    class PKeyError < OpenSSLError
    end

    # RSA is an asymmetric public key algorithm that has been formalized in
    # RFC 3447. It is in widespread use in public key infrastructures (PKI)
    # where certificates (cf. OpenSSL::X509::Certificate) often are issued
    # on the basis of a public/private RSA key pair. RSA is used in a wide
    # field of applications such as secure (symmetric) key exchange, e.g.
    # when establishing a secure TLS/SSL connection. It is also used in
    # various digital signature schemes.
    class RSA < PKey
      # Generates an RSA keypair.  _size_ is an integer representing the desired key
      # size.  Keys smaller than 1024 should be considered insecure.  _exponent_ is
      # an odd number normally 3, 17, or 65537.
      def self.generate(*several_variants) end

      # Generates or loads an RSA keypair.  If an integer _key_size_ is given it
      # represents the desired key size.  Keys less than 1024 bits should be
      # considered insecure.
      #
      # A key can instead be loaded from an _encoded_key_ which must be PEM or DER
      # encoded.  A _pass_phrase_ can be used to decrypt the key.  If none is given
      # OpenSSL will prompt for the pass phrase.
      #
      # = Examples
      #
      #   OpenSSL::PKey::RSA.new 2048
      #   OpenSSL::PKey::RSA.new File.read 'rsa.pem'
      #   OpenSSL::PKey::RSA.new File.read('rsa.pem'), 'my pass phrase'
      def initialize(*several_variants) end

      def blinding_off!; end

      def blinding_on!; end

      # Outputs this keypair in PEM encoding.  If _cipher_ and _pass_phrase_ are
      # given they will be used to encrypt the key.  _cipher_ must be an
      # OpenSSL::Cipher instance.
      def export(*args) end
      alias to_pem export
      alias to_s export

      def initialize_copy(p1) end

      # THIS METHOD IS INSECURE, PRIVATE INFORMATION CAN LEAK OUT!!!
      #
      # Stores all parameters of key to the hash.  The hash has keys 'n', 'e', 'd',
      # 'p', 'q', 'dmp1', 'dmq1', 'iqmp'.
      #
      # Don't use :-)) (It's up to you)
      def params; end

      # Does this keypair contain a private key?
      def private?; end

      # Decrypt _string_, which has been encrypted with the public key, with the
      # private key.  _padding_ defaults to PKCS1_PADDING.
      def private_decrypt(*several_variants) end

      # Encrypt _string_ with the private key.  _padding_ defaults to PKCS1_PADDING.
      # The encrypted string output can be decrypted using #public_decrypt.
      def private_encrypt(*several_variants) end

      # The return value is always +true+ since every private key is also a public
      # key.
      def public?; end

      # Decrypt _string_, which has been encrypted with the private key, with the
      # public key.  _padding_ defaults to PKCS1_PADDING.
      def public_decrypt(*several_variants) end

      # Encrypt _string_ with the public key.  _padding_ defaults to PKCS1_PADDING.
      # The encrypted string output can be decrypted using #private_decrypt.
      def public_encrypt(*several_variants) end

      # Makes new RSA instance containing the public key from the private key.
      def public_key; end

      # Sets _dmp1_, _dmq1_, _iqmp_ for the RSA instance. They are calculated by
      # <tt>d mod (p - 1)</tt>, <tt>d mod (q - 1)</tt> and <tt>q^(-1) mod p</tt>
      # respectively.
      def set_crt_params(dmp1, dmq1, iqmp) end

      # Sets _p_, _q_ for the RSA instance.
      def set_factors(p, q) end

      # Sets _n_, _e_, _d_ for the RSA instance.
      def set_key(n, e, d) end

      # Signs _data_ using the Probabilistic Signature Scheme (RSA-PSS) and returns
      # the calculated signature.
      #
      # RSAError will be raised if an error occurs.
      #
      # See #verify_pss for the verification operation.
      #
      # === Parameters
      # _digest_::
      #   A String containing the message digest algorithm name.
      # _data_::
      #   A String. The data to be signed.
      # _salt_length_::
      #   The length in octets of the salt. Two special values are reserved:
      #   +:digest+ means the digest length, and +:max+ means the maximum possible
      #   length for the combination of the private key and the selected message
      #   digest algorithm.
      # _mgf1_hash_::
      #   The hash algorithm used in MGF1 (the currently supported mask generation
      #   function (MGF)).
      #
      # === Example
      #   data = "Sign me!"
      #   pkey = OpenSSL::PKey::RSA.new(2048)
      #   signature = pkey.sign_pss("SHA256", data, salt_length: :max, mgf1_hash: "SHA256")
      #   pub_key = pkey.public_key
      #   puts pub_key.verify_pss("SHA256", signature, data,
      #                           salt_length: :auto, mgf1_hash: "SHA256") # => true
      def sign_pss(digest, data, salt_length:, mgf1_hash:) end

      # Outputs this keypair in DER encoding.
      def to_der; end

      # THIS METHOD IS INSECURE, PRIVATE INFORMATION CAN LEAK OUT!!!
      #
      # Dumps all parameters of a keypair to a String
      #
      # Don't use :-)) (It's up to you)
      def to_text; end

      # Verifies _data_ using the Probabilistic Signature Scheme (RSA-PSS).
      #
      # The return value is +true+ if the signature is valid, +false+ otherwise.
      # RSAError will be raised if an error occurs.
      #
      # See #sign_pss for the signing operation and an example code.
      #
      # === Parameters
      # _digest_::
      #   A String containing the message digest algorithm name.
      # _data_::
      #   A String. The data to be signed.
      # _salt_length_::
      #   The length in octets of the salt. Two special values are reserved:
      #   +:digest+ means the digest length, and +:auto+ means automatically
      #   determining the length based on the signature.
      # _mgf1_hash_::
      #   The hash algorithm used in MGF1.
      def verify_pss(digest, signature, data, salt_length:, mgf1_hash:) end
    end

    # Generic exception that is raised if an operation on an RSA PKey
    # fails unexpectedly or in case an instantiation of an instance of RSA
    # fails due to non-conformant input data.
    class RSAError < PKeyError
    end
  end

  module Random
    # Same as ::egd_bytes but queries 255 bytes by default.
    def self.egd(filename) end

    # Queries the entropy gathering daemon EGD on socket path given by _filename_.
    #
    # Fetches _length_ number of bytes and uses ::add to seed the OpenSSL built-in
    # PRNG.
    def self.egd_bytes(filename, length) end

    # Reads bytes from _filename_ and adds them to the PRNG.
    def self.load_random_file(filename) end

    # Generates a String with _length_ number of pseudo-random bytes.
    #
    # Pseudo-random byte sequences generated by ::pseudo_bytes will be unique if
    # they are of sufficient length, but are not necessarily unpredictable.
    #
    # === Example
    #
    #    OpenSSL::Random.pseudo_bytes(12)
    #    #=> "..."
    def self.pseudo_bytes(length) end

    # Mixes the bytes from _str_ into the Pseudo Random Number Generator(PRNG)
    # state.
    #
    # Thus, if the data from _str_ are unpredictable to an adversary, this
    # increases the uncertainty about the state and makes the PRNG output less
    # predictable.
    #
    # The _entropy_ argument is (the lower bound of) an estimate of how much
    # randomness is contained in _str_, measured in bytes.
    #
    # === Example
    #
    #    pid = $$
    #    now = Time.now
    #    ary = [now.to_i, now.nsec, 1000, pid]
    #    OpenSSL::Random.add(ary.join, 0.0)
    #    OpenSSL::Random.seed(ary.join)
    def self.random_add(p1, p2) end

    # Generates a String with _length_ number of cryptographically strong
    # pseudo-random bytes.
    #
    # === Example
    #
    #    OpenSSL::Random.random_bytes(12)
    #    #=> "..."
    def self.random_bytes(length) end

    # ::seed is equivalent to ::add where _entropy_ is length of _str_.
    def self.seed(str) end

    # Return +true+ if the PRNG has been seeded with enough data, +false+ otherwise.
    def self.status?; end

    # Writes a number of random generated bytes (currently 1024) to _filename_
    # which can be used to initialize the PRNG by calling ::load_random_file in a
    # later session.
    def self.write_random_file(filename) end

    private

    # Same as ::egd_bytes but queries 255 bytes by default.
    def egd(filename) end

    # Queries the entropy gathering daemon EGD on socket path given by _filename_.
    #
    # Fetches _length_ number of bytes and uses ::add to seed the OpenSSL built-in
    # PRNG.
    def egd_bytes(filename, length) end

    # Reads bytes from _filename_ and adds them to the PRNG.
    def load_random_file(filename) end

    # Generates a String with _length_ number of pseudo-random bytes.
    #
    # Pseudo-random byte sequences generated by ::pseudo_bytes will be unique if
    # they are of sufficient length, but are not necessarily unpredictable.
    #
    # === Example
    #
    #    OpenSSL::Random.pseudo_bytes(12)
    #    #=> "..."
    def pseudo_bytes(length) end

    # Mixes the bytes from _str_ into the Pseudo Random Number Generator(PRNG)
    # state.
    #
    # Thus, if the data from _str_ are unpredictable to an adversary, this
    # increases the uncertainty about the state and makes the PRNG output less
    # predictable.
    #
    # The _entropy_ argument is (the lower bound of) an estimate of how much
    # randomness is contained in _str_, measured in bytes.
    #
    # === Example
    #
    #    pid = $$
    #    now = Time.now
    #    ary = [now.to_i, now.nsec, 1000, pid]
    #    OpenSSL::Random.add(ary.join, 0.0)
    #    OpenSSL::Random.seed(ary.join)
    def random_add(p1, p2) end

    # Generates a String with _length_ number of cryptographically strong
    # pseudo-random bytes.
    #
    # === Example
    #
    #    OpenSSL::Random.random_bytes(12)
    #    #=> "..."
    def random_bytes(length) end

    # ::seed is equivalent to ::add where _entropy_ is length of _str_.
    def seed(str) end

    # Return +true+ if the PRNG has been seeded with enough data, +false+ otherwise.
    def status?; end

    # Writes a number of random generated bytes (currently 1024) to _filename_
    # which can be used to initialize the PRNG by calling ::load_random_file in a
    # later session.
    def write_random_file(filename) end

    class RandomError < OpenSSLError
    end
  end

  # Use SSLContext to set up the parameters for a TLS (former SSL)
  # connection. Both client and server TLS connections are supported,
  # SSLSocket and SSLServer may be used in conjunction with an instance
  # of SSLContext to set up connections.
  module SSL
    OP_ALL = _
    OP_ALLOW_NO_DHE_KEX = _
    OP_ALLOW_UNSAFE_LEGACY_RENEGOTIATION = _
    OP_CIPHER_SERVER_PREFERENCE = _
    OP_CISCO_ANYCONNECT = _
    OP_COOKIE_EXCHANGE = _
    OP_CRYPTOPRO_TLSEXT_BUG = _
    OP_DONT_INSERT_EMPTY_FRAGMENTS = _
    # Deprecated in OpenSSL 1.0.1k and 1.0.2.
    OP_EPHEMERAL_RSA = _
    OP_LEGACY_SERVER_CONNECT = _
    # Deprecated in OpenSSL 1.1.0.
    OP_MICROSOFT_BIG_SSLV3_BUFFER = _
    # Deprecated in OpenSSL 1.1.0.
    OP_MICROSOFT_SESS_ID_BUG = _
    # Deprecated in OpenSSL 0.9.7h and 0.9.8b.
    OP_MSIE_SSLV2_RSA_PADDING = _
    # Deprecated in OpenSSL 1.1.0.
    OP_NETSCAPE_CA_DN_BUG = _
    # Deprecated in OpenSSL 1.1.0.
    OP_NETSCAPE_CHALLENGE_BUG = _
    # Deprecated in OpenSSL 1.1.0.
    OP_NETSCAPE_DEMO_CIPHER_CHANGE_BUG = _
    # Deprecated in OpenSSL 0.9.8q and 1.0.0c.
    OP_NETSCAPE_REUSE_CIPHER_CHANGE_BUG = _
    OP_NO_COMPRESSION = _
    OP_NO_ENCRYPT_THEN_MAC = _
    OP_NO_QUERY_MTU = _
    OP_NO_RENEGOTIATION = _
    OP_NO_SESSION_RESUMPTION_ON_RENEGOTIATION = _
    # Deprecated in OpenSSL 1.1.0.
    OP_NO_SSLv2 = _
    OP_NO_SSLv3 = _
    OP_NO_TICKET = _
    OP_NO_TLSv1 = _
    OP_NO_TLSv1_1 = _
    OP_NO_TLSv1_2 = _
    OP_NO_TLSv1_3 = _
    # Deprecated in OpenSSL 1.0.1.
    OP_PKCS1_CHECK_1 = _
    # Deprecated in OpenSSL 1.0.1.
    OP_PKCS1_CHECK_2 = _
    OP_SAFARI_ECDHE_ECDSA_BUG = _
    # Deprecated in OpenSSL 1.1.0.
    OP_SINGLE_DH_USE = _
    # Deprecated in OpenSSL 1.1.0.
    OP_SINGLE_ECDH_USE = _
    # Deprecated in OpenSSL 1.1.0.
    OP_SSLEAY_080_CLIENT_DH_BUG = _
    # Deprecated in OpenSSL 1.0.1h and 1.0.2.
    OP_SSLREF2_REUSE_CERT_TYPE_BUG = _
    OP_TLSEXT_PADDING = _
    # Deprecated in OpenSSL 1.1.0.
    OP_TLS_BLOCK_PADDING_BUG = _
    # Deprecated in OpenSSL 1.1.0.
    OP_TLS_D5_BUG = _
    OP_TLS_ROLLBACK_BUG = _
    # SSL 2.0
    SSL2_VERSION = _
    # SSL 3.0
    SSL3_VERSION = _
    # TLS 1.1
    TLS1_1_VERSION = _
    # TLS 1.2
    TLS1_2_VERSION = _
    # TLS 1.3
    TLS1_3_VERSION = _
    # TLS 1.0
    TLS1_VERSION = _
    VERIFY_CLIENT_ONCE = _
    VERIFY_FAIL_IF_NO_PEER_CERT = _
    VERIFY_NONE = _
    VERIFY_PEER = _

    # An SSLContext is used to set various options regarding certificates,
    # algorithms, verification, session caching, etc.  The SSLContext is
    # used to create an SSLSocket.
    #
    # All attributes must be set before creating an SSLSocket as the
    # SSLContext will be frozen afterward.
    class SSLContext
      # Both client and server sessions are added to the session cache
      SESSION_CACHE_BOTH = _
      # Client sessions are added to the session cache
      SESSION_CACHE_CLIENT = _
      # Normally the session cache is checked for expired sessions every 255
      # connections.  Since this may lead to a delay that cannot be controlled,
      # the automatic flushing may be disabled and #flush_sessions can be
      # called explicitly.
      SESSION_CACHE_NO_AUTO_CLEAR = _
      # Enables both SESSION_CACHE_NO_INTERNAL_LOOKUP and
      # SESSION_CACHE_NO_INTERNAL_STORE.
      SESSION_CACHE_NO_INTERNAL = _
      # Always perform external lookups of sessions even if they are in the
      # internal cache.
      #
      # This flag has no effect on clients
      SESSION_CACHE_NO_INTERNAL_LOOKUP = _
      # Never automatically store sessions in the internal store.
      SESSION_CACHE_NO_INTERNAL_STORE = _
      # No session caching for client or server
      SESSION_CACHE_OFF = _
      # Server sessions are added to the session cache
      SESSION_CACHE_SERVER = _

      # Adds a certificate to the context. _pkey_ must be a corresponding private
      # key with _certificate_.
      #
      # Multiple certificates with different public key type can be added by
      # repeated calls of this method, and OpenSSL will choose the most appropriate
      # certificate during the handshake.
      #
      # #cert=, #key=, and #extra_chain_cert= are old accessor methods for setting
      # certificate and internally call this method.
      #
      # === Parameters
      # _certificate_::
      #   A certificate. An instance of OpenSSL::X509::Certificate.
      # _pkey_::
      #   The private key for _certificate_. An instance of OpenSSL::PKey::PKey.
      # _extra_certs_::
      #   Optional. An array of OpenSSL::X509::Certificate. When sending a
      #   certificate chain, the certificates specified by this are sent following
      #   _certificate_, in the order in the array.
      #
      # === Example
      #   rsa_cert = OpenSSL::X509::Certificate.new(...)
      #   rsa_pkey = OpenSSL::PKey.read(...)
      #   ca_intermediate_cert = OpenSSL::X509::Certificate.new(...)
      #   ctx.add_certificate(rsa_cert, rsa_pkey, [ca_intermediate_cert])
      #
      #   ecdsa_cert = ...
      #   ecdsa_pkey = ...
      #   another_ca_cert = ...
      #   ctx.add_certificate(ecdsa_cert, ecdsa_pkey, [another_ca_cert])
      #
      # === Note
      # OpenSSL before the version 1.0.2 could handle only one extra chain across
      # all key types. Calling this method discards the chain set previously.
      def add_certificate(p1, p2, p3 = v3) end

      # The list of cipher suites configured for this context.
      def ciphers; end

      # Sets the list of available cipher suites for this context.  Note in a server
      # context some ciphers require the appropriate certificates.  For example, an
      # RSA cipher suite can only be chosen when an RSA certificate is available.
      def ciphers=(p1) end

      # Sets the list of "supported elliptic curves" for this context.
      #
      # For a TLS client, the list is directly used in the Supported Elliptic Curves
      # Extension. For a server, the list is used by OpenSSL to determine the set of
      # shared curves. OpenSSL will pick the most appropriate one from it.
      #
      # Note that this works differently with old OpenSSL (<= 1.0.1). Only one curve
      # can be set, and this has no effect for TLS clients.
      #
      # === Example
      #   ctx1 = OpenSSL::SSL::SSLContext.new
      #   ctx1.ecdh_curves = "X25519:P-256:P-224"
      #   svr = OpenSSL::SSL::SSLServer.new(tcp_svr, ctx1)
      #   Thread.new { svr.accept }
      #
      #   ctx2 = OpenSSL::SSL::SSLContext.new
      #   ctx2.ecdh_curves = "P-256"
      #   cli = OpenSSL::SSL::SSLSocket.new(tcp_sock, ctx2)
      #   cli.connect
      #
      #   p cli.tmp_key.group.curve_name
      #   # => "prime256v1" (is an alias for NIST P-256)
      def ecdh_curves=(curve_list) end

      # Activate TLS_FALLBACK_SCSV for this context.
      # See RFC 7507.
      def enable_fallback_scsv; end

      # Removes sessions in the internal cache that have expired at _time_.
      def flush_sessions(time) end

      # Gets various OpenSSL options.
      def options; end

      # Sets various OpenSSL options.
      def options=(p1) end

      # Returns the security level for the context.
      #
      # See also OpenSSL::SSL::SSLContext#security_level=.
      def security_level; end

      # Sets the security level for the context. OpenSSL limits parameters according
      # to the level. The "parameters" include: ciphersuites, curves, key sizes,
      # certificate signature algorithms, protocol version and so on. For example,
      # level 1 rejects parameters offering below 80 bits of security, such as
      # ciphersuites using MD5 for the MAC or RSA keys shorter than 1024 bits.
      #
      # Note that attempts to set such parameters with insufficient security are
      # also blocked. You need to lower the level first.
      #
      # This feature is not supported in OpenSSL < 1.1.0, and setting the level to
      # other than 0 will raise NotImplementedError. Level 0 means everything is
      # permitted, the same behavior as previous versions of OpenSSL.
      #
      # See the manpage of SSL_CTX_set_security_level(3) for details.
      def security_level=(integer) end

      # Adds _session_ to the session cache.
      def session_add(session) end

      # The current session cache mode.
      def session_cache_mode; end

      # Sets the SSL session cache mode.  Bitwise-or together the desired
      # SESSION_CACHE_* constants to set.  See SSL_CTX_set_session_cache_mode(3) for
      # details.
      def session_cache_mode=(integer) end

      # Returns the current session cache size.  Zero is used to represent an
      # unlimited cache size.
      def session_cache_size; end

      # Sets the session cache size.  Returns the previously valid session cache
      # size.  Zero is used to represent an unlimited session cache size.
      def session_cache_size=(integer) end

      # Returns a Hash containing the following keys:
      #
      # :accept:: Number of started SSL/TLS handshakes in server mode
      # :accept_good:: Number of established SSL/TLS sessions in server mode
      # :accept_renegotiate:: Number of start renegotiations in server mode
      # :cache_full:: Number of sessions that were removed due to cache overflow
      # :cache_hits:: Number of successfully reused connections
      # :cache_misses:: Number of sessions proposed by clients that were not found
      #                 in the cache
      # :cache_num:: Number of sessions in the internal session cache
      # :cb_hits:: Number of sessions retrieved from the external cache in server
      #            mode
      # :connect:: Number of started SSL/TLS handshakes in client mode
      # :connect_good:: Number of established SSL/TLS sessions in client mode
      # :connect_renegotiate:: Number of start renegotiations in client mode
      # :timeouts:: Number of sessions proposed by clients that were found in the
      #             cache but had expired due to timeouts
      def session_cache_stats; end

      # Removes _session_ from the session cache.
      def session_remove(session) end

      # This method is called automatically when a new SSLSocket is created.
      # However, it is not thread-safe and must be called before creating
      # SSLSocket objects in a multi-threaded program.
      def setup; end
      alias freeze setup

      private

      # Sets the minimum and maximum supported protocol versions. See #min_version=
      # and #max_version=.
      def set_minmax_proto_version(min, max) end
    end

    # Generic error class raised by SSLSocket and SSLContext.
    class SSLError < OpenSSLError
    end

    class SSLErrorWaitReadable < SSLError
      include IO::WaitReadable
    end

    class SSLErrorWaitWritable < SSLError
      include IO::WaitWritable
    end

    class SSLSocket
      # Creates a new SSL socket from _io_ which must be a real IO object (not an
      # IO-like object that responds to read/write).
      #
      # If _ctx_ is provided the SSL Sockets initial params will be taken from
      # the context.
      #
      # The OpenSSL::Buffering module provides additional IO methods.
      #
      # This method will freeze the SSLContext if one is provided;
      # however, session management is still allowed in the frozen SSLContext.
      def initialize(*several_variants) end

      # Waits for a SSL/TLS client to initiate a handshake.  The handshake may be
      # started after unencrypted data has been sent over the socket.
      def accept; end

      # Initiates the SSL/TLS handshake as a server in non-blocking manner.
      #
      #   # emulates blocking accept
      #   begin
      #     ssl.accept_nonblock
      #   rescue IO::WaitReadable
      #     IO.select([s2])
      #     retry
      #   rescue IO::WaitWritable
      #     IO.select(nil, [s2])
      #     retry
      #   end
      #
      # By specifying a keyword argument _exception_ to +false+, you can indicate
      # that accept_nonblock should not raise an IO::WaitReadable or
      # IO::WaitWritable exception, but return the symbol +:wait_readable+ or
      # +:wait_writable+ instead.
      def accept_nonblock(*options) end

      # Returns the ALPN protocol string that was finally selected by the server
      # during the handshake.
      def alpn_protocol; end

      # The X509 certificate for this socket endpoint.
      def cert; end

      # Returns the cipher suite actually used in the current session, or nil if
      # no session has been established.
      def cipher; end

      # Returns the list of client CAs. Please note that in contrast to
      # SSLContext#client_ca= no array of X509::Certificate is returned but
      # X509::Name instances of the CA's subject distinguished name.
      #
      # In server mode, returns the list set by SSLContext#client_ca=.
      # In client mode, returns the list of client CAs sent from the server.
      def client_ca; end

      # Initiates an SSL/TLS handshake with a server.  The handshake may be started
      # after unencrypted data has been sent over the socket.
      def connect; end

      # Initiates the SSL/TLS handshake as a client in non-blocking manner.
      #
      #   # emulates blocking connect
      #   begin
      #     ssl.connect_nonblock
      #   rescue IO::WaitReadable
      #     IO.select([s2])
      #     retry
      #   rescue IO::WaitWritable
      #     IO.select(nil, [s2])
      #     retry
      #   end
      #
      # By specifying a keyword argument _exception_ to +false+, you can indicate
      # that connect_nonblock should not raise an IO::WaitReadable or
      # IO::WaitWritable exception, but return the symbol +:wait_readable+ or
      # +:wait_writable+ instead.
      def connect_nonblock(*options) end

      # Sets the server hostname used for SNI. This needs to be set before
      # SSLSocket#connect.
      def hostname=(hostname) end

      # Returns the protocol string that was finally selected by the client
      # during the handshake.
      def npn_protocol; end

      # The X509 certificate for this socket's peer.
      def peer_cert; end

      # The X509 certificate chain for this socket's peer.
      def peer_cert_chain; end

      # The number of bytes that are immediately available for reading.
      def pending; end

      # Sets the Session to be used when the connection is established.
      def session=(session) end

      # Returns +true+ if a reused session was negotiated during the handshake.
      def session_reused?; end

      # Returns a String representing the SSL/TLS version that was negotiated
      # for the connection, for example "TLSv1.2".
      def ssl_version; end

      # A description of the current connection state. This is for diagnostic
      # purposes only.
      def state; end

      # Reads _length_ bytes from the SSL connection.  If a pre-allocated _buffer_
      # is provided the data will be written into it.
      def sysread(*several_variants) end

      # Writes _string_ to the SSL connection.
      def syswrite(string) end

      # Returns the ephemeral key used in case of forward secrecy cipher.
      def tmp_key; end

      # Returns the result of the peer certificates verification.  See verify(1)
      # for error values and descriptions.
      #
      # If no peer certificate was presented X509_V_OK is returned.
      def verify_result; end

      private

      # Sends "close notify" to the peer and tries to shut down the SSL connection
      # gracefully.
      def stop; end

      # A non-blocking version of #sysread.  Raises an SSLError if reading would
      # block.  If "exception: false" is passed, this method returns a symbol of
      # :wait_readable, :wait_writable, or nil, rather than raising an exception.
      #
      # Reads _length_ bytes from the SSL connection.  If a pre-allocated _buffer_
      # is provided the data will be written into it.
      def sysread_nonblock(*several_variants) end

      # Writes _string_ to the SSL connection in a non-blocking manner.  Raises an
      # SSLError if writing would block.
      def syswrite_nonblock(string) end
    end

    class Session
      # Creates a new Session object from an instance of SSLSocket or DER/PEM encoded
      # String.
      def initialize(*several_variants) end

      # Returns +true+ if the two Session is the same, +false+ if not.
      def ==(other) end

      # Returns the Session ID.
      def id; end

      def initialize_copy(p1) end

      # Returns the time at which the session was established.
      def time; end

      # Sets start time of the session. Time resolution is in seconds.
      def time=(*several_variants) end

      # Returns the timeout value set for the session, in seconds from the
      # established time.
      def timeout; end

      # Sets how long until the session expires in seconds.
      def timeout=(integer) end

      # Returns an ASN1 encoded String that contains the Session object.
      def to_der; end

      # Returns a PEM encoded String that contains the Session object.
      def to_pem; end

      # Shows everything in the Session object. This is for diagnostic purposes.
      def to_text; end

      class SessionError < OpenSSLError
      end
    end
  end

  module X509
    class Attribute
      def initialize(p1, p2 = v2) end

      def initialize_copy(p1) end

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

      def initialize_copy(p1) end

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

    # Implementation of an X.509 certificate as specified in RFC 5280.
    # Provides access to a certificate's attributes and allows certificates
    # to be read from a string, but also supports the creation of new
    # certificates from scratch.
    #
    # === Reading a certificate from a file
    #
    # Certificate is capable of handling DER-encoded certificates and
    # certificates encoded in OpenSSL's PEM format.
    #
    #   raw = File.read "cert.cer" # DER- or PEM-encoded
    #   certificate = OpenSSL::X509::Certificate.new raw
    #
    # === Saving a certificate to a file
    #
    # A certificate may be encoded in DER format
    #
    #   cert = ...
    #   File.open("cert.cer", "wb") { |f| f.print cert.to_der }
    #
    # or in PEM format
    #
    #   cert = ...
    #   File.open("cert.pem", "wb") { |f| f.print cert.to_pem }
    #
    # X.509 certificates are associated with a private/public key pair,
    # typically a RSA, DSA or ECC key (see also OpenSSL::PKey::RSA,
    # OpenSSL::PKey::DSA and OpenSSL::PKey::EC), the public key itself is
    # stored within the certificate and can be accessed in form of an
    # OpenSSL::PKey. Certificates are typically used to be able to associate
    # some form of identity with a key pair, for example web servers serving
    # pages over HTTPs use certificates to authenticate themselves to the user.
    #
    # The public key infrastructure (PKI) model relies on trusted certificate
    # authorities ("root CAs") that issue these certificates, so that end
    # users need to base their trust just on a selected few authorities
    # that themselves again vouch for subordinate CAs issuing their
    # certificates to end users.
    #
    # The OpenSSL::X509 module provides the tools to set up an independent
    # PKI, similar to scenarios where the 'openssl' command line tool is
    # used for issuing certificates in a private PKI.
    #
    # === Creating a root CA certificate and an end-entity certificate
    #
    # First, we need to create a "self-signed" root certificate. To do so,
    # we need to generate a key first. Please note that the choice of "1"
    # as a serial number is considered a security flaw for real certificates.
    # Secure choices are integers in the two-digit byte range and ideally
    # not sequential but secure random numbers, steps omitted here to keep
    # the example concise.
    #
    #   root_key = OpenSSL::PKey::RSA.new 2048 # the CA's public/private key
    #   root_ca = OpenSSL::X509::Certificate.new
    #   root_ca.version = 2 # cf. RFC 5280 - to make it a "v3" certificate
    #   root_ca.serial = 1
    #   root_ca.subject = OpenSSL::X509::Name.parse "/DC=org/DC=ruby-lang/CN=Ruby CA"
    #   root_ca.issuer = root_ca.subject # root CA's are "self-signed"
    #   root_ca.public_key = root_key.public_key
    #   root_ca.not_before = Time.now
    #   root_ca.not_after = root_ca.not_before + 2 * 365 * 24 * 60 * 60 # 2 years validity
    #   ef = OpenSSL::X509::ExtensionFactory.new
    #   ef.subject_certificate = root_ca
    #   ef.issuer_certificate = root_ca
    #   root_ca.add_extension(ef.create_extension("basicConstraints","CA:TRUE",true))
    #   root_ca.add_extension(ef.create_extension("keyUsage","keyCertSign, cRLSign", true))
    #   root_ca.add_extension(ef.create_extension("subjectKeyIdentifier","hash",false))
    #   root_ca.add_extension(ef.create_extension("authorityKeyIdentifier","keyid:always",false))
    #   root_ca.sign(root_key, OpenSSL::Digest::SHA256.new)
    #
    # The next step is to create the end-entity certificate using the root CA
    # certificate.
    #
    #   key = OpenSSL::PKey::RSA.new 2048
    #   cert = OpenSSL::X509::Certificate.new
    #   cert.version = 2
    #   cert.serial = 2
    #   cert.subject = OpenSSL::X509::Name.parse "/DC=org/DC=ruby-lang/CN=Ruby certificate"
    #   cert.issuer = root_ca.subject # root CA is the issuer
    #   cert.public_key = key.public_key
    #   cert.not_before = Time.now
    #   cert.not_after = cert.not_before + 1 * 365 * 24 * 60 * 60 # 1 years validity
    #   ef = OpenSSL::X509::ExtensionFactory.new
    #   ef.subject_certificate = cert
    #   ef.issuer_certificate = root_ca
    #   cert.add_extension(ef.create_extension("keyUsage","digitalSignature", true))
    #   cert.add_extension(ef.create_extension("subjectKeyIdentifier","hash",false))
    #   cert.sign(root_key, OpenSSL::Digest::SHA256.new)
    class Certificate
      def initialize(*several_variants) end

      # Compares the two certificates. Note that this takes into account all fields,
      # not just the issuer name and the serial number.
      def ==(other) end

      def add_extension(extension) end

      # Returns +true+ if _key_ is the corresponding private key to the Subject
      # Public Key Information, +false+ otherwise.
      def check_private_key(key) end

      def extensions; end

      def extensions=(p1) end

      def initialize_copy(p1) end

      def inspect; end

      def issuer; end

      def issuer=(name) end

      def not_after; end

      def not_after=(time) end

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

      # Verifies the signature of the certificate, with the public key _key_. _key_
      # must be an instance of OpenSSL::PKey.
      def verify(key) end

      def version; end

      def version=(integer) end
    end

    class CertificateError < OpenSSLError
    end

    class Extension
      # Creates an X509 extension.
      #
      # The extension may be created from _der_ data or from an extension _oid_
      # and _value_.  The _oid_ may be either an OID or an extension name.  If
      # _critical_ is +true+ the extension is marked critical.
      def initialize(*several_variants) end

      def critical=(p1) end

      def critical?; end

      def initialize_copy(p1) end

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

      # Creates a new X509::Extension with passed values. See also x509v3_config(5).
      def create_ext(*several_variants) end

      def crl=(p1) end

      def issuer_certificate=(p1) end

      def subject_certificate=(p1) end

      def subject_request=(p1) end
    end

    # An X.509 name represents a hostname, email address or other entity
    # associated with a public key.
    #
    # You can create a Name by parsing a distinguished name String or by
    # supplying the distinguished name as an Array.
    #
    #   name = OpenSSL::X509::Name.parse 'CN=nobody/DC=example'
    #
    #   name = OpenSSL::X509::Name.new [['CN', 'nobody'], ['DC', 'example']]
    class Name
      include Comparable

      # A flag for #to_s.
      #
      # Breaks the name returned into multiple lines if longer than 80
      # characters.
      COMPAT = _
      # The default object type for name entries.
      DEFAULT_OBJECT_TYPE = _
      # A flag for #to_s.
      #
      # Returns a multiline format.
      MULTILINE = _
      # The default object type template for name entries.
      OBJECT_TYPE_TEMPLATE = _
      # A flag for #to_s.
      #
      # Returns a more readable format than RFC2253.
      ONELINE = _
      # A flag for #to_s.
      #
      # Returns an RFC2253 format name.
      RFC2253 = _

      # Creates a new Name.
      #
      # A name may be created from a DER encoded string _der_, an Array
      # representing a _distinguished_name_ or a _distinguished_name_ along with a
      # _template_.
      #
      #   name = OpenSSL::X509::Name.new [['CN', 'nobody'], ['DC', 'example']]
      #
      #   name = OpenSSL::X509::Name.new name.to_der
      #
      # See add_entry for a description of the _distinguished_name_ Array's
      # contents
      def initialize(*several_variants) end

      # Adds a new entry with the given _oid_ and _value_ to this name.  The _oid_
      # is an object identifier defined in ASN.1.  Some common OIDs are:
      #
      # C::  Country Name
      # CN:: Common Name
      # DC:: Domain Component
      # O::  Organization Name
      # OU:: Organizational Unit Name
      # ST:: State or Province Name
      #
      # The optional keyword parameters _loc_ and _set_ specify where to insert the
      # new attribute. Refer to the manpage of X509_NAME_add_entry(3) for details.
      # _loc_ defaults to -1 and _set_ defaults to 0. This appends a single-valued
      # RDN to the end.
      def add_entry(p1, p2, p3 = v3, p4 = {}) end

      # Compares this Name with _other_ and returns +0+ if they are the same and +-1+
      # or ++1+ if they are greater or less than each other respectively.
      def cmp(other) end
      alias <=> cmp

      # Returns true if _name_ and _other_ refer to the same hash key.
      def eql?(other) end

      # The hash value returned is suitable for use as a certificate's filename in
      # a CA path.
      def hash; end

      # Returns an MD5 based hash used in OpenSSL 0.9.X.
      def hash_old; end

      def initialize_copy(p1) end

      # Returns an Array representation of the distinguished name suitable for
      # passing to ::new
      def to_a; end

      # Converts the name to DER encoding
      def to_der; end

      # Returns a String representation of the Distinguished Name. _format_ is
      # one of:
      #
      # * OpenSSL::X509::Name::COMPAT
      # * OpenSSL::X509::Name::RFC2253
      # * OpenSSL::X509::Name::ONELINE
      # * OpenSSL::X509::Name::MULTILINE
      #
      # If _format_ is omitted, the largely broken and traditional OpenSSL format
      # is used.
      def to_s(*several_variants) end

      # Returns an UTF-8 representation of the distinguished name, as specified
      # in {RFC 2253}[https://www.ietf.org/rfc/rfc2253.txt].
      def to_utf8; end
    end

    class NameError < OpenSSLError
    end

    class Request
      def initialize(p1 = v1) end

      def add_attribute(p1) end

      def attributes; end

      def attributes=(p1) end

      def initialize_copy(p1) end

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

      def initialize_copy(p1) end

      def serial; end

      def serial=(p1) end

      def time; end

      def time=(p1) end

      def to_der; end
    end

    class RevokedError < OpenSSLError
    end

    # The X509 certificate store holds trusted CA certificates used to verify
    # peer certificates.
    #
    # The easiest way to create a useful certificate store is:
    #
    #   cert_store = OpenSSL::X509::Store.new
    #   cert_store.set_default_paths
    #
    # This will use your system's built-in certificates.
    #
    # If your system does not have a default set of certificates you can obtain
    # a set extracted from Mozilla CA certificate store by cURL maintainers
    # here: https://curl.haxx.se/docs/caextract.html (You may wish to use the
    # firefox-db2pem.sh script to extract the certificates from a local install
    # to avoid man-in-the-middle attacks.)
    #
    # After downloading or generating a cacert.pem from the above link you
    # can create a certificate store from the pem file like this:
    #
    #   cert_store = OpenSSL::X509::Store.new
    #   cert_store.add_file 'cacert.pem'
    #
    # The certificate store can be used with an SSLSocket like this:
    #
    #   ssl_context = OpenSSL::SSL::SSLContext.new
    #   ssl_context.verify_mode = OpenSSL::SSL::VERIFY_PEER
    #   ssl_context.cert_store = cert_store
    #
    #   tcp_socket = TCPSocket.open 'example.com', 443
    #
    #   ssl_socket = OpenSSL::SSL::SSLSocket.new tcp_socket, ssl_context
    class Store
      # Creates a new X509::Store.
      def initialize; end

      # Adds the OpenSSL::X509::Certificate _cert_ to the certificate store.
      def add_cert(cert) end

      # Adds the OpenSSL::X509::CRL _crl_ to the store.
      def add_crl(crl) end

      # Adds the certificates in _file_ to the certificate store. _file_ is the path
      # to the file, and the file contains one or more certificates in PEM format
      # concatenated together.
      def add_file(file) end

      # Adds _path_ as the hash dir to be looked up by the store.
      def add_path(path) end

      # Sets _flags_ to the Store. _flags_ consists of zero or more of the constants
      # defined in with name V_FLAG_* or'ed together.
      def flags=(flags) end

      # Sets the store's purpose to _purpose_. If specified, the verifications on
      # the store will check every untrusted certificate's extensions are consistent
      # with the purpose. The purpose is specified by constants:
      #
      # * X509::PURPOSE_SSL_CLIENT
      # * X509::PURPOSE_SSL_SERVER
      # * X509::PURPOSE_NS_SSL_SERVER
      # * X509::PURPOSE_SMIME_SIGN
      # * X509::PURPOSE_SMIME_ENCRYPT
      # * X509::PURPOSE_CRL_SIGN
      # * X509::PURPOSE_ANY
      # * X509::PURPOSE_OCSP_HELPER
      # * X509::PURPOSE_TIMESTAMP_SIGN
      def purpose=(purpose) end

      # Configures _store_ to look up CA certificates from the system default
      # certificate store as needed basis. The location of the store can usually be
      # determined by:
      #
      # * OpenSSL::X509::DEFAULT_CERT_FILE
      # * OpenSSL::X509::DEFAULT_CERT_DIR
      def set_default_paths; end

      # Sets the time to be used in verifications.
      def time=(time) end

      def trust=(trust) end

      # Performs a certificate verification on the OpenSSL::X509::Certificate _cert_.
      #
      # _chain_ can be an array of OpenSSL::X509::Certificate that is used to
      # construct the certificate chain.
      #
      # If a block is given, it overrides the callback set by #verify_callback=.
      #
      # After finishing the verification, the error information can be retrieved by
      # #error, #error_string, and the resulting complete certificate chain can be
      # retrieved by #chain.
      def verify(cert, chain = nil) end

      # General callback for OpenSSL verify
      def verify_callback=(p1) end
    end

    # A StoreContext is used while validating a single certificate and holds
    # the status involved.
    class StoreContext
      def initialize(store, cert = nil, chain = nil) end

      def chain; end

      def current_cert; end

      def current_crl; end

      def error; end

      def error=(error_code) end

      def error_depth; end

      # Returns the error string corresponding to the error code retrieved by #error.
      def error_string; end

      # Sets the verification flags to the context. See Store#flags=.
      def flags=(flags) end

      # Sets the purpose of the context. See Store#purpose=.
      def purpose=(purpose) end

      # Sets the time used in the verification. If not set, the current time is used.
      def time=(time) end

      def trust=(trust) end

      def verify; end
    end

    class StoreError < OpenSSLError
    end
  end
end

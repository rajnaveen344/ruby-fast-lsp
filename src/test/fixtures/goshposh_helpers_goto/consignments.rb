require 'platform/helpers/util'
require 'platform/state_machines/consignment_request_machine'
require 'platform/state_machines/consignment_package_machine'
require 'platform/consignments/consignment_presentation_helper'
require 'platform/consignments/consignment_comms_helper'
require 'platform/consignments/consignment_chat_helper'
require 'platform/consignments/consignment_schedule_helper'
require 'platform/consignments/consignment_partner_matching_helper'
require 'platform/consignments/consignment_helper'
require 'platform/helpers/index_helpers'
require 'platform/users/user_qr_code'
require 'rqrcode'

module GoshPosh::Platform::API
  module Consignments
    include ConsignmentPresentationHelper
    include GoshPosh::Platform::Consignments::ConsignmentCommsHelper
    include GoshPosh::Platform::Consignments::ConsignmentChatHelper
    include GoshPosh::Platform::Consignments::ConsignmentScheduleHelper
    include GoshPosh::Platform::Consignments::ConsignmentPartnerMatchingHelper
    include GoshPosh::Platform::Consignments::ConsignmentHelper
    include GoshPosh::Platform::IndexHelpers

    PARTNER_FIELDS = [
      :id,
      :consignment_supplier_id,
      :state,
      :aggregates,
      :consignment_partner_address,
      :partner_shipment_delivery_info
    ].freeze

    SUPPLIER_FIELDS = [
      :id,
      :consignment_partner_id,
      :state,
      :aggregates,
      :consignment_supplier_address,
      :supplier_shipment_collection_info
    ].freeze

    PACKAGE_SENDER_FIELDS = [
      :id,
      :state,
      :consignment_supplier_id,
      :consignment_package_sender_id,
      :package_label_id,
      :created_at
    ].freeze

    SUPPLIER_AGGREGATES_FIELDS = %i[total_items sold_items total_supplier_earnings_amount supplier_unread_chat_messages_count].freeze
    PARTNER_AGGREGATES_FIELDS = %i[total_items sold_items discarded_items total_supplier_earnings_amount total_partner_earnings_amount partner_unread_chat_messages_count].freeze

    CONSIGNMENT_PACKAGE_FIELDS = [
      :package_label_id
    ].freeze

    POST_FIELDS = [
      :id,
      :title,
      :price_amount,
      :inventory,
      :size_obj,
      :cover_shot
    ].freeze

    SUPPLIER_ENROLLMENT_FIELDS = %i[id status user_id created_at].freeze
    PARTNER_ENROLLMENT_FIELDS = %i[id status user_id address_notes created_at].freeze

    PICKUP_SCHEDULED_MESSAGE = 'Pickup is scheduled for %{day}, %{date} between %{time}.'.freeze
    PICKUP_INITIATED_MESSAGE = 'Arriving today at %{time}'.freeze
    PACKAGE_IN_TRANSIT_MESSAGE = 'Hang tight! Your Closet Partner will receive your bag soon.'.freeze
    PENDING_INVENTORY_MESSAGE = 'Hang tight! Your Closet Partner will start listing soon.'.freeze
    CANCELLED_REQUEST_MESSAGE = 'This bag is no longer active.'.freeze
    LAUNCHED_WAITLIST_TITLE = "You're on the list!".freeze
    LAUNCHED_WAITLIST_MESSAGE = "We're experiencing high demand in your area, so hang tight—we'll be in touch as soon as we're ready for you.".freeze
    WAITLIST_TITLE = 'Thanks for your interest!'.freeze
    WAITLIST_MESSAGE = "We'll notify you with next steps when we launch in your area.".freeze
    ENROLLMENT_CLOSED_TITLE = 'Thank you for your interest.'.freeze
    ENROLLMENT_CLOSED_MESSAGE = 'We are evolving our Consignment program and are not currently scheduling new bags at this time.'.freeze
    CONSIGNMENT_LAUNCHED_MESSAGE = "We've launched in your area!".freeze
    CONSIGNMENT_LAUNCHING_SOON_MESSAGE = "We're launching in your area soon!".freeze
    GET_STARTED_LABEL = 'Get Started'.freeze
    NOTIFY_WHEN_AVAILABLE_LABEL = 'Notify When Available'.freeze
    JOIN_WAITLIST_LABEL = 'Join Waitlist'.freeze
    REQUEST_BAG_LABEL = 'Request Bag'.freeze

    PARTNER_ENROLLMENT_SUBMITTED_BANNER_TITLE = 'Thanks for signing up to sell for others!'.freeze
    PARTNER_ENROLLMENT_SUBMITTED_BANNER_MESSAGE = "We will reach out if you are selected to become a Closet Partner.\n\nWatch a tutorial to learn about selling for others in the meantime.".freeze

    def supplier_consignment_request(access_context, user_id, consignment_request_id, recent_posts_count)
      user_id = check_access(access_context[:access_token], user_id)

      check_consignment_request_access(access_context[:home_domain])

      supplier_id = user_id
      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)

      if consignment_request.nil? || consignment_request[:consignment_supplier_id] != supplier_id
        raise GoshPosh::Platform::Errors::ConsignmentRequestNotFoundError.new
      end

      consignment_request_details = consignment_request.slice(*SUPPLIER_FIELDS)
      consignment_request_details[:aggregates] = consignment_request_details[:aggregates]
                                                   .slice(*SUPPLIER_AGGREGATES_FIELDS)
      consignment_request_details.merge!(
        consignment_display_status(consignment_request, GoshPosh::Platform::ConsignmentActor::SUPPLIER)
      )
      consignment_request_details.merge!(consignment_package_details(consignment_request))
      consignment_request_details.merge!(consignment_state_transition_details(consignment_request))

      consignment_user_references_cache = consignment_user_references(
        [consignment_request], [:consignment_supplier_id, :consignment_partner_id]
      )
      consignment_request_details[:consignment_supplier_user_info] = consignment_user_references_cache[
        consignment_request[:consignment_supplier_id]
      ]
      consignment_request_details[:consignment_partner_user_info] = consignment_user_references_cache[
        consignment_request[:consignment_partner_id]
      ]

      cover_shots = consignment_cover_shots([consignment_request])
      consignment_request_details[:cover_shot] = cover_shots[consignment_request[:id]]

      recent_post_ids = consignment_request[:post_ids].last(recent_posts_count)
      has_more_posts = (recent_posts_count < consignment_request[:post_ids].length)
      consignment_posts = consignment_posts(access_context, recent_post_ids)
      consignment_request_details[:recent_posts] = consignment_posts[:visible_posts]
      consignment_request_details[:hidden_posts] = consignment_posts[:hidden_posts]

      consignment_request_details[:aggregates][:total_items] -= consignment_posts[:hidden_posts].length


      if consignment_request[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_PICKUP_INITIATED
        label = services.shipping_service.get_latest_consignment_shipping_label(
          consignment_request[:id],
          nil,
          [ GoshPosh::Platform::Commerce::ShippingLabelState::ACTIVE,
            GoshPosh::Platform::Commerce::ShippingLabelState::USED ],
          [ GoshPosh::Platform::Commerce::ShippingLabelReason::COMPLETE_CONSIGNMENT_REQUEST ]
        )

        driver_data_token = label.dig(:shipping_method_info, :delivery_info, :courier, :driver_data_token) if label
        if driver_data_token && !driver_data_token.empty?
          encoded_driver_data = GoshPosh::Platform::Vault::VaultClient.load(
            driver_data_token,
            GoshPosh::Platform::Users::IdentityVerificationInfoVaultKey::DRIVER_DATA,
            GoshPosh::Platform::VaultNames::LETHE_90D)
          driver_info = Marshal.load(Base64.decode64(encoded_driver_data)) if encoded_driver_data
          courier_info = { driver: driver_info,
                           vehicle: label.dig(:shipping_method_info, :delivery_info, :courier, :vehicle) }
          consignment_request_details[:latest_shipping_label] = {
            courier: courier_info,
            expected_pickup_at: label[:expected_pickup_at]
          }
        end
      elsif [GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SHIPPED_TO_SUPPLIER,
             GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_DELIVERED_TO_SUPPLIER].include?(consignment_request[:state])
        label = services.shipping_service.get_latest_consignment_shipping_label(
          consignment_request[:id],
          nil,
          [GoshPosh::Platform::Commerce::ShippingLabelState::ACTIVE,
            GoshPosh::Platform::Commerce::ShippingLabelState::USED],
          [GoshPosh::Platform::Commerce::ShippingLabelReason::CONSIGNMENT_PMHQ_TO_PCS]
        )
        if label
          consignment_request_details[:latest_shipping_label] = {
            expected_delivery_at: label[:expected_delivery_at],
            tracking_number: label[:tracking_number],
            tracking_url: GoshPosh::Platform::Util.get_shipping_label_tracking_url(
              label[:carrier],
              label[:tracking_number],
              GoshPosh::Platform::Commerce::TrackingLinkUserRole::RECEIVER
            )
          }
        end
      end

      chat_data = chat_data_for_consignment_request_details(
        supplier_id, consignment_request, GoshPosh::Platform::ConsignmentActor::SUPPLIER
      )
      consignment_request_details.merge!(chat_data[:data])

      phone_number_id = services.user_service.get_phone_number_id_for_user(
        consignment_request[:consignment_supplier_id]
      )
      if phone_number_id
        phone_number_details = {
          phone_number_id: phone_number_id,
          phone_number: services.user_service.get_masked_phone_number_for_id(phone_number_id)
        }
        consignment_request_details.merge!(phone_number_details)
      end


      meta = { has_more_posts: has_more_posts, total_posts_count: consignment_request[:post_ids].length }
      meta.merge!(chat_data[:meta])

      {
        data: consignment_request_details,
        meta: meta,
        presentation: supplier_consignment_request_presentation(
          access_context,
          consignment_request_details.merge(
            {
              state_history: consignment_request[:state_history],
              home_domain: access_context[:home_domain],
              supplier_shipment_collection_info_history: consignment_request[:supplier_shipment_collection_info_history]
            }
          )
        )
      }
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::SUPPLIER_VIEW_CONSIGNMENT_REQUEST_ERROR,
        __FILE__,
        __method__,
        { consignment_request_id: consignment_request_id } # attrs
      )

      raise
    end

    def partner_consignment_request(access_context, user_id, consignment_request_id, recent_posts_count)
      user_id = check_access(access_context[:access_token], user_id)

      check_consignment_request_access(access_context[:home_domain])

      partner_id = user_id
      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)

      if consignment_request.nil? || consignment_request[:consignment_partner_id] != partner_id
        raise GoshPosh::Platform::Errors::ConsignmentRequestNotFoundError.new
      end

      consignment_request_details = consignment_request.slice(*PARTNER_FIELDS)
      consignment_request_details[:aggregates] = consignment_request_details[:aggregates]
                                                   .slice(*PARTNER_AGGREGATES_FIELDS)
      consignment_request_details.merge!(
        consignment_display_status(consignment_request, GoshPosh::Platform::ConsignmentActor::PARTNER)
      )
      consignment_request_details.merge!(consignment_package_details(consignment_request))
      consignment_request_details.merge!(consignment_state_transition_details(consignment_request))

      consignment_user_references_cache = consignment_user_references(
        [consignment_request], [:consignment_supplier_id, :consignment_partner_id]
      )
      consignment_request_details[:consignment_supplier_user_info] = consignment_user_references_cache[
        consignment_request[:consignment_supplier_id]
      ]
      consignment_request_details[:consignment_partner_user_info] = consignment_user_references_cache[
        consignment_request[:consignment_partner_id]
      ]

      if consignment_request[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SHIPPED_TO_PARTNER
        label = services.shipping_service.get_latest_consignment_shipping_label(
          consignment_request[:id],
          nil,
          [ GoshPosh::Platform::Commerce::ShippingLabelState::ACTIVE,
            GoshPosh::Platform::Commerce::ShippingLabelState::USED ],
          [ GoshPosh::Platform::Commerce::ShippingLabelReason::COMPLETE_CONSIGNMENT_REQUEST ]
        )
        if label
          consignment_request_details[:latest_shipping_label] = {
            tracking_url: label[:external_tracking_url],
            expected_delivery_at: label[:expected_delivery_at]
          }
          driver_data_token = label.dig(:shipping_method_info, :delivery_info, :courier, :driver_data_token)
          if driver_data_token && !driver_data_token.empty?
            encoded_driver_data = GoshPosh::Platform::Vault::VaultClient.load(
              driver_data_token,
              GoshPosh::Platform::Users::IdentityVerificationInfoVaultKey::DRIVER_DATA,
              GoshPosh::Platform::VaultNames::LETHE_90D)
            driver_info = Marshal.load(Base64.decode64(encoded_driver_data)) if encoded_driver_data
            consignment_request_details[:latest_shipping_label][:courier] = {
              driver: driver_info,
              vehicle: label.dig(:shipping_method_info, :delivery_info, :courier, :vehicle)
            }
          end
        end
      end

      cover_shots = consignment_cover_shots([consignment_request])
      consignment_request_details[:cover_shot] = cover_shots[consignment_request[:id]]

      is_mobile_app_type = GoshPosh::Platform::AppTypes::MOBILE_TYPES.include?(
        GoshPosh::Platform::Util.app_platform_from_type(access_context[:app_type], false)
      )

      is_old_app_version = GoshPosh::Platform::Util.is_app_version_between_range(
        access_context[:app_version], nil, GoshPosh::Settings.consignment_posts_attr_app_max_version
      )

      if is_mobile_app_type && is_old_app_version
        has_more_posts = false
        consignment_posts = consignment_posts(access_context, consignment_request[:post_ids])
        consignment_request_details[:posts] = consignment_posts[:visible_posts]
      else
        recent_post_ids = consignment_request[:post_ids].last(recent_posts_count)
        has_more_posts = (recent_posts_count < consignment_request[:post_ids].length)
        consignment_posts = consignment_posts(access_context, recent_post_ids)
        consignment_request_details[:recent_posts] = consignment_posts[:visible_posts]
      end

      consignment_request_details[:hidden_posts] = consignment_posts[:hidden_posts]
      consignment_request_details[:aggregates][:total_items] -= consignment_posts[:hidden_posts].length

      chat_data = chat_data_for_consignment_request_details(
        partner_id, consignment_request, GoshPosh::Platform::ConsignmentActor::PARTNER
      )
      consignment_request_details.merge!(chat_data[:data])

      supplier_stats = services.user_service.get_user_aggregates(consignment_request[:consignment_supplier_id])
      consignment_request_details.merge!(
        { consignment_supplier_requests_count: supplier_stats[:consignment_supplier_requests_count] }
      )

      meta = { has_more_posts: has_more_posts, total_posts_count: consignment_request[:post_ids].length }
      meta.merge!(chat_data[:meta])

      {
        data: consignment_request_details,
        meta: meta,
        presentation: partner_consignment_request_presentation(
          consignment_request_details.merge({ state_history: consignment_request[:state_history] })
        )
      }
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::PARTNER_VIEW_CONSIGNMENT_REQUEST_ERROR,
        __FILE__,
        __method__,
        { consignment_request_id: consignment_request_id } # attrs
      )

      raise
    end

    def supplier_consignment_requests(access_context, user_id, max_id, count, recent_posts_count)
      request_max_id = max_id
      user_id = check_access(access_context[:access_token], user_id)
      check_consignment_request_access(access_context[:home_domain])

      supplier_id = user_id

      consignment_requests = []

      # TODO (Naveen): Filter requests by ES once es filter is implemented
      loop do
        request_ids = services.consignment_service.get_supplier_request_ids(supplier_id, max_id, count)

        unless request_ids.empty?
          consignment_requests.concat services.consignment_service.get_consignment_requests_details(
            request_ids, GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_VISIBLE_STATES
          )
        end

        break if consignment_requests.length >= count || request_ids.empty?

        max_id = request_ids.last
      end

      consignment_requests = consignment_requests.first(count)

      consignment_partner_references = consignment_user_references(consignment_requests, [:consignment_partner_id])

      consignment_post_references = {}
      consignment_post_ids = consignment_requests.map { |r| r[:post_ids] }.flatten
      consignment_posts = consignment_posts(access_context, consignment_post_ids).dig(:visible_posts)
      consignment_posts.each { |post| consignment_post_references[post[:id]] = post }

      no_posts_message = {}
      cancelled_request_message = {}

      data = consignment_requests.map do |consignment_request|
        consignment_request_details = consignment_request.slice(*SUPPLIER_FIELDS)
        consignment_request_details[:aggregates] = consignment_request_details[:aggregates]
                                                     .slice(*SUPPLIER_AGGREGATES_FIELDS)

        hidden_posts = consignment_posts(
          access_context, consignment_request[:post_ids], [GoshPosh::Platform::Posts::PostStatus::HIDDEN]
        ).dig(:hidden_posts)
        consignment_request_details[:aggregates][:total_items] -= hidden_posts.length

        consignment_request_details[:recent_posts] = consignment_request[:post_ids]
                                                       .last(recent_posts_count).reverse.map do |post_id|
          consignment_post_references[post_id]
        end.compact

        is_old_app_version = GoshPosh::Platform::Util.is_app_version_between_range(
          access_context[:app_version], nil, GoshPosh::Settings.consignment_v7_old_states_app_max_version
        )
        if is_old_app_version
          transform_states_for_older_apps!(consignment_request_details)
        end

        consignment_request_details.merge!(
          consignment_display_status(consignment_request, GoshPosh::Platform::ConsignmentActor::SUPPLIER)
        )
        consignment_request_details.merge!(consignment_package_details(consignment_request))
        consignment_request_details.merge!(consignment_state_transition_details(consignment_request))
        consignment_request_details.merge!(supplier_consignment_requests_partner_info(
                                             consignment_request,
                                             consignment_partner_references
                                           ))

        if consignment_request_details[:recent_posts].empty?
          case consignment_request_details[:state]
          when GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_DELIVERED_TO_SUPPLIER,
            GoshPosh::Platform::Consignments::ConsignmentRequestState::AWAITING_PICKUP_SCHEDULE
            delivered_state_change = consignment_request[:state_history].find do |state_change|
              state_change[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_DELIVERED_TO_SUPPLIER
            end
            delivered_on = delivered_state_change&.dig(:created_at)
            delivered_on = delivered_on.iso8601 if delivered_on
            consignment_request_details[:delivered_on] = delivered_on
          when GoshPosh::Platform::Consignments::ConsignmentRequestState::PICKUP_SCHEDULED,
               GoshPosh::Platform::Consignments::ConsignmentRequestState::PARTNER_ASSIGNED
            collection_info = consignment_request[:supplier_shipment_collection_info]
            if collection_info && collection_info[:pickup_start_at] && collection_info[:pickup_end_at]
              pickup_start_at = collection_info[:pickup_start_at].localtime
              pickup_end_at = collection_info[:pickup_end_at].localtime
              day = if pickup_start_at.to_date == Date.today
                      'today'
                    elsif pickup_start_at.to_date == Date.tomorrow
                      'tomorrow'
                    else
                      pickup_start_at.strftime('%A')
                    end
              time = "#{pickup_start_at.strftime('%l %p')} - " \
                "#{pickup_end_at.strftime('%l %p')}"
              date = pickup_end_at.strftime('%B %d')
              message = format(PICKUP_SCHEDULED_MESSAGE, day: day, date: date, time: time)
            else
              message= PACKAGE_IN_TRANSIT_MESSAGE
            end
            no_posts_message[consignment_request[:id]] = { message: message }
          when GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_PICKUP_INITIATED
            label = services.shipping_service.get_latest_consignment_shipping_label(
              consignment_request[:id],
              nil,
              [ GoshPosh::Platform::Commerce::ShippingLabelState::ACTIVE,
                GoshPosh::Platform::Commerce::ShippingLabelState::USED ],
              [ GoshPosh::Platform::Commerce::ShippingLabelReason::COMPLETE_CONSIGNMENT_REQUEST ]
            )
            time = label[:expected_pickup_at].strftime('%l %p')
            no_posts_message[consignment_request[:id]] = { message: format(PICKUP_INITIATED_MESSAGE, time: time) }
            consignment_request_details[:expected_pickup_at] = label[:expected_pickup_at].iso8601 if label[:expected_pickup_at]
          when GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SHIPPED_TO_PARTNER
            no_posts_message[consignment_request[:id]] = { message: PACKAGE_IN_TRANSIT_MESSAGE }
          else
            no_posts_message[consignment_request[:id]] = { message: PENDING_INVENTORY_MESSAGE }
          end
        end

        if consignment_request_details[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::CANCELLED
          cancelled_request_message[consignment_request[:id]] = { message: CANCELLED_REQUEST_MESSAGE }
        end

        consignment_request_details[:show_schedule_pickup_option] = can_supplier_update_pickup_window?(
          consignment_request_details
        )
        consignment_request_details[:show_delivery_info] = show_pickup_info_in_supplier_consignment_requests_list_page?(
          consignment_request_details
        )

        consignment_request_details
      end.compact

      res = { data: data }
      res[:meta] = { next_max_id: data.last[:id] } unless data.empty?
      res[:presentation] =
        supplier_consignment_requests_sections_presentation(access_context, user_id, request_max_id, data)
          .merge({
            no_posts_message: no_posts_message,
            cancelled_request_message: cancelled_request_message
          })

      res
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::SUPPLIER_VIEW_CONSIGNMENT_REQUESTS_LIST_ERROR,
        __FILE__,
        __method__
      )

      raise
    end

    def partner_consignment_requests(access_context, user_id, max_id, count)
      initial_max_id = max_id
      user_id = check_access(access_context[:access_token], user_id)

      check_consignment_request_access(access_context[:home_domain])

      partner_id = user_id

      consignment_requests = []

      # TODO (Naveen): Filter requests by ES once es filter is implemented
      loop do
        request_ids, max_id = services.consignment_service.get_partner_request_ids(partner_id, max_id, count)

        unless request_ids.empty?
          services.consignment_service.get_consignment_requests_details(request_ids).each do |request|
            if request[:consignment_partner_id] || GoshPosh::Platform::Consignments::ConsignmentRequestState::PARTNER_VISIBLE_STATES.include?(request[:state])
              consignment_requests << request
            end
          end
        end

        break if consignment_requests.length >= count || request_ids.empty?
      end

      consignment_requests = consignment_requests.first(count)

      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        access_context[:home_domain]
      )

      if consignment_fs && consignment_fs[:partner_consignment_requests_sorting]
        if initial_max_id.nil?
          consignment_requests = consignment_requests.sort_by do |request|
            delivered_state = request[:state_history]&.find { |entry| entry[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::PENDING_INVENTORY_PROCESSING }
            created_at = delivered_state&.dig(:created_at)

            [delivered_state ? 0 : 1, -(created_at ? created_at.to_i : 0)]
          end
        end
      end

      consignment_supplier_references = consignment_user_references(consignment_requests, [:consignment_supplier_id])
      cover_shots = consignment_cover_shots(consignment_requests)

      data = consignment_requests.map do |consignment_request|
        consignment_request_details = consignment_request.slice(*PARTNER_FIELDS)

        consignment_request_details.merge!(
          consignment_display_status(consignment_request, GoshPosh::Platform::ConsignmentActor::PARTNER)
        )

        hidden_posts = consignment_posts(
          access_context, consignment_request[:post_ids], [GoshPosh::Platform::Posts::PostStatus::HIDDEN]
        ).dig(:hidden_posts)
        consignment_request_details[:aggregates][:total_items] -= hidden_posts.length

        consignment_request_details.merge!(consignment_package_details(consignment_request))
        consignment_request_details.merge!(consignment_state_transition_details(consignment_request))

        consignment_request_details[:consignment_supplier_user_info] = consignment_supplier_references[
          consignment_request[:consignment_supplier_id]
        ]
        consignment_request_details[:cover_shot] = cover_shots[consignment_request[:id]]

        consignment_request_details
      end.compact

      res = { data: data }
      res[:meta] = { next_max_id: max_id } unless data.empty?
      res[:presentation] = partner_consignment_requests_presentation(access_context, user_id)

      res
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::PARTNER_VIEW_CONSIGNMENT_REQUESTS_LIST_ERROR,
        __FILE__,
        __method__
      )

      raise
    end

    def package_sender_consignment_requests(access_context, user_id, max_id, count)
      user_id = check_access(access_context[:access_token], user_id)

      check_consignment_request_access(access_context[:home_domain])

      package_sender_id = user_id

      consignment_requests = []

      loop do
        request_ids, max_id = services.consignment_service.get_package_sender_request_ids(package_sender_id, max_id, count)

        unless request_ids.empty?
          consignment_requests.concat services.consignment_service.get_consignment_requests_details(
            request_ids, GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SENDER_VISIBLE_STATES
          )
        end

        consignment_requests.reject! do |consignment_request|
          is_package_sender = consignment_request[:consignment_package_sender_id].to_s == user_id.to_s

          !is_package_sender || request_cancelled_before_package_sender_shipped?(consignment_request)
        end

        break if consignment_requests.length >= count || request_ids.empty?
      end

      consignment_requests = consignment_requests.first(count)

      consignment_supplier_references = consignment_user_references(consignment_requests, [:consignment_supplier_id])
      cover_shots = consignment_cover_shots(consignment_requests)

      data = consignment_requests.map do |consignment_request|
        consignment_request_details = consignment_request.slice(*PACKAGE_SENDER_FIELDS)

        consignment_request_details.merge!(
          consignment_display_status(consignment_request, GoshPosh::Platform::ConsignmentActor::PACKAGE_SENDER)
        )

        consignment_request_details.merge!(consignment_state_transition_details(consignment_request))

        consignment_request_details[:consignment_supplier_user_info] = consignment_supplier_references[
          consignment_request[:consignment_supplier_id]
        ]
        consignment_request_details[:cover_shot] = cover_shots[consignment_request[:id]]

        consignment_request_details
      end.compact

      res = { data: data }
      res[:meta] = { next_max_id: max_id } unless data.empty?
      res[:presentation] = package_sender_consignment_requests_presentation(access_context, package_sender_id)

      res
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::PACKAGE_SENDER_VIEW_CONSIGNMENT_REQUESTS_LIST_ERROR,
        __FILE__,
        __method__
      )

      raise error
    end

    def get_consignment_request_all_posts(access_context, user_id, consignment_request_id)
      user_id = check_access(access_context[:access_token], user_id)

      check_consignment_request_access(access_context[:home_domain])

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)

      raise GoshPosh::Platform::Errors::ConsignmentRequestNotFoundError.new if consignment_request.nil?

      if consignment_request.nil? ||
         (consignment_request[:consignment_partner_id] != user_id &&
         consignment_request[:consignment_supplier_id] != user_id)
        raise GoshPosh::Platform::Errors::ConsignmentRequestNotFoundError.new
      end

      { data: consignment_posts(access_context, consignment_request[:post_ids]).dig(:visible_posts) }
    end

    def update_consignment_request_state(access_context, user_id, consignment_request_id, state, params = {})
      user_id = check_access(access_context[:access_token], user_id)

      state = state.to_sym
      consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      )
      consignment_request = consignment_request_machine.consignment_request

      if consignment_request.nil? ||
         (consignment_request[:consignment_supplier_id] != user_id && consignment_request[:consignment_partner_id] != user_id &&
           consignment_request[:consignment_package_sender_id] != user_id)
        raise GoshPosh::Platform::Errors::ConsignmentRequestNotFoundError.new
      end

      consignment_supplier = consignment_request[:consignment_supplier_id] == user_id
      consignment_partner = consignment_request[:consignment_partner_id] == user_id

      case state
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SENDER_MARKED_AS_SHIPPED
        unless consignment_request[:consignment_package_sender_id] == user_id
          raise GoshPosh::Platform::Errors::InvalidConsignmentRequestStateError
        end

        return unless consignment_request[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::AWAITING_PACKAGE_SENDER_SHIPMENT

        consignment_request_machine.package_sender_marked_shipped(access_context)
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_DELIVERED_TO_SUPPLIER
        shipped_to_supplier_state_change = consignment_request[:state_history].find do |state_change|
          state_change[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SHIPPED_TO_SUPPLIER
        end

        bag_shipped_to_supplier_at = shipped_to_supplier_state_change&.dig(:created_at)
        supplier_home_domain = get_user_home_domain(access_context, user_id)
        consignment_shipping_fs = GoshPosh::FeatureSettings.get_domain_based(
          GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_SHIPPING,
          supplier_home_domain
        )

        if Time.now < (bag_shipped_to_supplier_at + consignment_shipping_fs[:enable_mark_as_delivered_threshold_in_seconds])
          time_in_hours = consignment_shipping_fs[:enable_mark_as_delivered_threshold_in_seconds] / 1.hour
          raise GoshPosh::Platform::Errors::InvalidConsignmentRequestStateError.new(
            "The bag can be marked as received #{time_in_hours} hours after it has been shipped."
          )
        end

        consignment_request_machine.package_delivered_to_supplier(access_context)
        consignment_request_machine.await_pickup_schedule(access_context)
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::PICKUP_SCHEDULED
        raise GoshPosh::Platform::Errors::InvalidConsignmentRequestStateError unless consignment_supplier

        unless [
          GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SHIPPED_TO_SUPPLIER,
          GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_DELIVERED_TO_SUPPLIER,
          GoshPosh::Platform::Consignments::ConsignmentRequestState::AWAITING_PICKUP_SCHEDULE
        ].include?(consignment_request[:state])
          # Post V8.2: Support for older apps
          return
        end

        consignment_request = consignment_request_machine.consignment_request
        scheduled_partner_id = schedule_a_partner_for_consignment_request(access_context, consignment_request)

        consignment_request_machine.schedule_pickup(access_context, scheduled_partner_id)
        consignment_request = consignment_request_machine.consignment_request
        services.db_queue_service.push_consignment_request_to_pickup_trigger_queue(
          consignment_request_id,
          consignment_request[:supplier_shipment_collection_info][:pickup_start_at]
        )

        consignment_partner_matched_notify_partner_comms(scheduled_partner_id, consignment_request, consignment_request[:supplier_shipment_collection_info][:pickup_start_at])
        schedule_supplier_pickup_reminder_comms(consignment_request)
        schedule_supplier_bag_inventory_reminder_comms(consignment_request)
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::INVENTORY_PROCESSED
        raise GoshPosh::Platform::Errors::InvalidConsignmentRequestStateError unless consignment_partner

        consignment_request_machine.complete_inventory_processing(access_context)
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::CANCELLED
        raise GoshPosh::Platform::Errors::InvalidConsignmentRequestStateError unless consignment_supplier

        cancel_reason = params[:cancel_reason]&.to_sym
        if GoshPosh::Platform::Consignments::ConsignmentRequestCancelReason::SUPPLIER_CANCEL_REASONS
           .exclude?(cancel_reason)
          raise GoshPosh::Platform::Errors::InvalidInputError.new('Invalid cancel reason')
        end

        begin
          consignment_request_machine.cancel_consignment_request(
            access_context,
            GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_CANCEL_ALLOWED_STATES,
            cancel_reason,
            params[:cancel_reason_note]
          )
        rescue
          error_message =
            if cancel_reason == GoshPosh::Platform::Consignments::ConsignmentRequestCancelReason::SUPPLIER_NEVER_RECEIVED_BAG
              GoshPosh::Platform::Errors::ConsignmentErrorMessages::BAG_NOT_YET_DELIVERED
            else
              GoshPosh::Platform::Errors::ConsignmentErrorMessages::UNABLE_TO_CANCEL_BAG
            end

          raise GoshPosh::Platform::Errors::InvalidConsignmentRequestStateError.new(error_message)
        end
      else
        raise GoshPosh::Platform::Errors::InvalidInputError.new('Invalid state')
      end
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::UPDATE_CONSIGNMENT_REQUEST_STATUS_ERROR,
        __FILE__,
        __method__,
        {
          consignment_request_id: consignment_request_id,
          user_id: user_id,
          state: state
        } # attrs
      )

      raise error
    end

    def schedule_supplier_bag_inventory_reminder_comms(consignment_request)
      begin
        pickup_created_at = consignment_request.dig(:supplier_shipment_collection_info, :pickup_created_at)
        pickup_start_at = consignment_request.dig(:supplier_shipment_collection_info, :pickup_start_at)
        days_until_pickup = (pickup_start_at.to_date - Time.now.to_date).to_i

        if days_until_pickup <= 7
          run_at = pickup_created_at + 1.hour
        else
          pickup_day = pickup_start_at.beginning_of_day.ago(7.days)
          run_at = pickup_day.advance(hours: Time.parse(GoshPosh::Settings.consignment_comms_start_threshold_at).hour)
        end

        services.db_queue_service.push_message_to_supplier_bag_inventory_reminder_queue(
          consignment_request[:id],
          run_at
        )
      rescue => e
        services.logger.warn(
          GoshPosh::Platform::Util.print_stack_trace(
            "#{__method__} - Failed scheduling supplier bag inventory reminder for CR #{consignment_request[:id]}",
            e
          )
        )
      end
    end

    def consignment_request_as_admin(access_context, consignment_request_id, include_posts: false, include_shipping_labels: false)
      check_admin_access(access_context[:access_token])

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)

      if consignment_request[:consignment_package_id]
        consignment_package = services.consignment_service.get_consignment_package_details(
          consignment_request[:consignment_package_id]
        )
      end

      supplier = get_user_as_admin(access_context[:access_token], consignment_request[:consignment_supplier_id])

      if consignment_request[:consignment_package_sender_id]
        package_sender = get_user_as_admin(access_context[:access_token], consignment_request[:consignment_package_sender_id])
      end

      if consignment_request[:consignment_partner_id]
        partner = get_user_as_admin(access_context[:access_token], consignment_request[:consignment_partner_id])
      end

      if include_posts
        post_ids = consignment_request[:post_ids]

        posts = services.post_service.posts_by_ids_v2(
          post_ids, GoshPosh::Platform::Posts::PostStatus::CONSIGNMENT_POST_STATUSES
        )

        # using "post_ids" to build the array to preserve the order
        ordered_posts = post_ids.map { |post_id| posts[post_id] }.compact
      end

      if include_shipping_labels
        shipping_labels = services.shipping_service.get_shipping_labels_of_consignment_request(consignment_request_id)
      end

      consignment_supplier_images = services.consignment_service.get_consignment_request_inventory_images(consignment_request_id)

      {
        consignment_request: consignment_request,
        consignment_package: consignment_package,
        supplier: supplier,
        package_sender: package_sender,
        consignment_supplier_images: consignment_supplier_images,
        partner: partner,
        posts: ordered_posts,
        shipping_labels: shipping_labels
      }
    end

    def consignment_package_sender_requests_for_admin(access_context, user_id, max_id, count)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      consignment_request_ids, max_id = services.consignment_service.get_package_sender_request_ids(user_id, max_id, count)
      consignment_requests = services.consignment_service.get_consignment_requests_details(consignment_request_ids)
      consignment_references = consignment_user_references(consignment_requests, %i[consignment_supplier_id consignment_partner_id])
      data = admin_consignment_requests_summary(consignment_requests, consignment_references)

      {
        data: data,
        meta: { next_max_id: max_id }
      }
    end

    # TO DO: pagination support
    def consignment_supplier_requests_for_admin(access_context, user_id, max_id, count)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      consignment_request_ids = services.consignment_service.get_supplier_request_ids(user_id, max_id, count)
      consignment_requests = services.consignment_service.get_consignment_requests_details(consignment_request_ids)
      consignment_references = consignment_user_references(consignment_requests, %i[consignment_partner_id consignment_package_sender_id])
      data = admin_consignment_requests_summary(consignment_requests, consignment_references)

      {
        data: data,
        meta: { next_max_id: consignment_request_ids.last }
      }
    end

    # TO DO: pagination support
    def consignment_partner_requests_for_admin(access_context, user_id, max_id, count)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      consignment_request_ids, max_id = services.consignment_service.get_partner_request_ids(user_id, max_id, count)
      consignment_requests = services.consignment_service.get_consignment_requests_details(consignment_request_ids)
      consignment_references = consignment_user_references(consignment_requests, %i[consignment_supplier_id consignment_package_sender_id])
      data = admin_consignment_requests_summary(consignment_requests, consignment_references)

      {
        data: data,
        meta: { next_max_id: max_id }
      }
    end

    def get_consignment_supplier_sold_orders_for_admin(access_context, user_id, bucket_number)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])
      if bucket_number.nil?
        commerce_info = services.order_service.get_user_latest_commerce_info(user_id)
        if commerce_info && commerce_info[:consignment_supplier_sold_orders_page_no]
          bucket_number = commerce_info[:consignment_supplier_sold_orders_page_no]
        end
      end
      order_ids = services.order_service.get_user_consignment_supplier_sold_order_ids(user_id, PmModel::Index::DESCENDING, bucket_number)
      unless order_ids.empty?
        order_details = populate_order_info(user_id, order_ids, GoshPosh::Platform::ConsignmentActor::SUPPLIER)
        order_details.each do |item|
          if item[:ind_status]
            item[:sr_id] = services.order_service.get_support_requests_for_order(item[:order_id]).first[:id]
          end
        end
      end
      bucket_number = bucket_number.nil? ? -1 : (bucket_number - 1)
      { order_details: order_details, next_order_bucket_number: bucket_number }
    end

    def new_consignment_request_data_for_admin(access_context, user_id)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      consignment_supplier = get_user_as_admin(access_context[:access_token], user_id)
      consignment_supplier_address =
        begin
          get_consignment_request_address(user_id)
        rescue
          nil
        end
      {
        consignment_supplier: consignment_supplier,
        consignment_supplier_address: consignment_supplier_address
      }
    end

    def create_consignment_request(access_context, consignment_request_data, opts = {})
      # TODO: Too complex, split into smaller methods for each caller type
      caller_is_admin = opts[:caller_is_admin] || false
      caller_is_system = opts[:caller_is_system] || false
      caller_is_user = opts[:caller_is_user] || false

      if caller_is_admin
        check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])
      elsif caller_is_system
        check_system_access(access_context)
      elsif caller_is_user
        check_access(access_context[:access_token], consignment_request_data[:consignment_supplier_id])
      else
        raise GoshPosh::Platform::Errors::InvalidInputError.new('Invalid caller')
      end

      supplier_home_domain = get_user_home_domain(nil, consignment_request_data[:consignment_supplier_id])

      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        supplier_home_domain
      )

      if consignment_request_data[:consignment_supplier_address].blank?
        consignment_request_data[:consignment_supplier_address] = get_consignment_request_address(
          consignment_request_data[:consignment_supplier_id]
        )
      end

      validate_and_update_consignment_request_addresses!(supplier_home_domain, consignment_request_data)

      supplier_scanned_package = # Future: partner scanning
        if consignment_request_data[:package_label_id] && caller_is_user
          validate_and_get_package_for_assignment(consignment_request_data[:package_label_id])
        end
      previous_request = get_latest_request_for_supplier(consignment_request_data[:consignment_supplier_id])

      if previous_request && GoshPosh::Platform::Consignments::ConsignmentRequestState::QUICK_CHECKOUT_OPEN_STATES.include?(
        previous_request[:state]
      )
        consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
          services, services.logger, previous_request[:id]
        )
        consignment_request_machine.clear_request(access_context)
        previous_request = consignment_request_machine.consignment_request
      end

      if previous_request&.dig(:state) == GoshPosh::Platform::Consignments::ConsignmentRequestState::NEW
        # Update new address in the previous request
        if consignment_request_data[:consignment_supplier_address]
          services.consignment_service.update_consignment_request_on_address_change(
            previous_request[:id],
            consignment_request_data[:consignment_supplier_address],
            [GoshPosh::Platform::Consignments::ConsignmentRequestState::NEW],
            previous_request[:notify_capacity_availability]
          )
        end

        # If the user abandons a request checkout flow and tries to create a new request,
        # don't create a new request in db. Return the previous "new" request.
        if supplier_scanned_package
          GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
            services, services.logger, previous_request[:id]
          ).supplier_assign_package(access_context, supplier_scanned_package)
        end
        if caller_is_user
          if consignment_fs[:skip_package_checkout_confirmation]
            consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
              services, services.logger, previous_request[:id]
            )
            consignment_request_machine.submit_request(access_context)
            consignment_request_machine.approve_request(access_context)
          end

          return supplier_consignment_request(
            access_context,
            consignment_request_data[:consignment_supplier_id],
            previous_request[:id],
            GoshPosh::Settings.max_consignment_recent_posts_count
          )
        end
      end

      consignment_request_data[:previous_pickup_notes] = previous_request
                                                           &.dig(:supplier_shipment_collection_info, :pickup_notes)

      check_consignment_request_creation(supplier_home_domain)
      unless caller_is_admin
        validate_consignment_request_creation(
          access_context, consignment_request_data[:consignment_supplier_id], consignment_fs
        )
      end

      if previous_request&.dig(:state) == GoshPosh::Platform::Consignments::ConsignmentRequestState::NEW
        consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(services, services.logger, previous_request[:id])
      else
        consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(services)
        consignment_request_machine.create_user_request(access_context, consignment_request_data)
      end

      if caller_is_user
        if supplier_scanned_package
          consignment_request_machine.supplier_assign_package(access_context, supplier_scanned_package)
        end

        if consignment_fs[:skip_package_checkout_confirmation]
          consignment_request_machine.submit_request(access_context)
          consignment_request_machine.approve_request(access_context)
        end

        return supplier_consignment_request(
          access_context,
          consignment_request_data[:consignment_supplier_id],
          consignment_request_machine.consignment_request[:id],
          GoshPosh::Settings.max_consignment_recent_posts_count
        )
      end

      consignment_request_machine.submit_request(access_context)
      consignment_request_machine.approve_request(access_context)

      if consignment_fs[:empty_package_sender_enabled_v2]
        consignment_request_machine.initiate_request_for_package_sender_assignment(access_context)
      elsif consignment_fs[:empty_package_sender_enabled] && (consignment_request_data[:package_with_supplier].nil? || !consignment_request_data[:package_with_supplier])
        consignment_request_machine.initiate_request_for_package_sender_assignment(access_context)
        reserve_empty_consignment_package_sender(
          access_context, consignment_request_machine.consignment_request
        )&.tap do |package_sender_id|
          consignment_request_machine.assign_package_sender(access_context, package_sender_id)
        end
      else
        consignment_request_machine.initiate_request(access_context)
      end

      # Assign package if package_label_id is present
      if consignment_request_data[:package_label_id].present?
        assign_consignment_package_as_admin(
          access_context,
          consignment_request_machine.consignment_request[:id],
          consignment_request_data[:package_label_id]
        )

        if consignment_request_data[:package_with_supplier].present? &&
           consignment_request_data[:package_with_supplier].to_s == 'true'
          consignment_request_machine.package_delivered_to_supplier(access_context)
          consignment_request_machine.await_pickup_schedule(access_context)
        end
      end

      # Assign partner if consignment_partner_username is present
      if consignment_request_data[:package_label_id].present? &&
         consignment_request_data[:consignment_partner_username].present?
        assign_consignment_partner_as_admin(
          access_context,
          consignment_request_machine.consignment_request[:id],
          consignment_request_data
        )
      end

      consignment_request_machine.consignment_request.slice(:id)
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::ADMIN_CREATE_CONSIGNMENT_REQUEST_ERROR,
        __FILE__,
        __method__
      )

      raise
    end

    def submit_consignment_request(access_context, consignment_request_id)
      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      check_access(access_context[:access_token], consignment_request[:consignment_supplier_id])

      consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      )

      consignment_request_machine.submit_request(access_context)
      consignment_request_machine.approve_request(access_context)

      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        access_context[:home_domain]
      )
      return if consignment_fs[:skip_package_checkout_confirmation]

      if consignment_fs[:empty_package_sender_enabled_v2]
        consignment_request_machine.initiate_request_for_package_sender_assignment(access_context)
      elsif  consignment_fs[:empty_package_sender_enabled]
        consignment_request_machine.initiate_request_for_package_sender_assignment(access_context)
        reserve_empty_consignment_package_sender(
          access_context, consignment_request_machine.consignment_request
        )&.tap do |package_sender_id|
          consignment_request_machine.assign_package_sender(access_context, package_sender_id)
        end
      else
        consignment_request_machine.initiate_request(access_context)
      end
    end

    def clear_consignment_request(access_context, consignment_request_id)
      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      check_access(access_context[:access_token], consignment_request[:consignment_supplier_id])

      GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      ).clear_request(access_context)
    end

    def assign_consignment_package_as_admin(access_context, consignment_request_id, package_label_id)
      if access_context[:system] && access_context[:user_id] == GoshPosh::Platform::POSHMARK_ID
        # Allow POSHMARK_ID to create consignment requests
      else
        check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])
      end

      package = validate_and_get_consignment_package(package_label_id)
      consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      )
      consignment_request_machine.assign_consignment_package(access_context, package[:id], package[:package_label_id])

      consignment_request = consignment_request_machine.consignment_request
      services.consignment_service.update_consignment_package_request_id(
        consignment_request[:consignment_package_id],
        consignment_request[:id],
        [
          GoshPosh::Platform::Consignments::ConsignmentPackageState::NEW,
          GoshPosh::Platform::Consignments::ConsignmentPackageState::INACTIVE
        ]
      )
    end

    def assign_consignment_partner_as_admin(access_context, consignment_request_id, partner_data)
      if access_context[:system] && access_context[:user_id] == GoshPosh::Platform::POSHMARK_ID
        # Allow POSHMARK_ID to create consignment requests
      else
        check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])
      end

      if partner_data[:consignment_partner_username].nil? || partner_data[:consignment_partner_username].empty? ||
         partner_data[:consignment_partner_address].nil? || partner_data[:consignment_partner_address].empty?
        raise GoshPosh::Platform::Errors::InvalidInputError.new('required partner username and partner address')
      end

      consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      )

      consignment_request = consignment_request_machine.consignment_request
      partner_username = partner_data[:consignment_partner_username]
      partner = get_user_by_handle(partner_username)
      partner_info = services.consignment_service.consignment_partner_info(partner[:id])
      dropoff_notes = partner_info[:address_notes] if partner_info
      partner_address = GoshPosh::Platform::Util.normalize_address(partner_data[:consignment_partner_address])
      partner_address = calculate_address_coordinates(partner[:id], partner_address, update_address_book: false)
      validate_consignment_partner_address(partner_address, consignment_request)
      validate_consignment_partner(partner, consignment_request)

      consignment_request_machine.assign_partner(
        access_context,
        partner[:id],
        partner_address,
        dropoff_notes: dropoff_notes,
        trigger: :admin
      )
    end

    def assign_consignment_package_sender_as_admin(access_context, consignment_request_id, package_sender_username)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      if package_sender_username.blank?
        raise GoshPosh::Platform::Errors::InvalidInputError.new('Required package sender username')
      end

      new_package_sender = get_user_by_handle(package_sender_username)

      if new_package_sender.nil?
        raise GoshPosh::Platform::Errors::InvalidInputError.new('Invalid package sender username')
      end

      consignment_address = services.order_service.default_consignment_address_in_user_address_list(new_package_sender[:id])
      unless consignment_address && consignment_address[:coordinates]&.any?
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          'Package sender does not have a proper Consignment Address'
        )
      end

      new_package_sender_info = services.consignment_service.consignment_partner_info(new_package_sender[:id])
      if new_package_sender_info.nil?
        raise GoshPosh::Platform::Errors::InvalidInputError.new('Package sender info not found')
      end

      unless new_package_sender_info[:empty_package_sender_enabled]
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          'The package sender does not have the empty package sender feature enabled'
        )
      end

      if new_package_sender_info[:empty_package_available_count].zero?
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          'The package sender does not have any empty packages available'
        )
      end

      request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      )
      consignment_request = request_machine.consignment_request
      old_package_sender_id = consignment_request[:consignment_package_sender_id]

      if old_package_sender_id
        if old_package_sender_id == new_package_sender[:id]
          raise GoshPosh::Platform::Errors::InvalidInputError.new(
            'New package sender cannot be same as the old package sender'
          )
        end

        GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
          services, services.logger, consignment_request[:id]
        ).unassign_package_sender(access_context, old_package_sender_id)
      end
      previous_state = consignment_request[:state]

      last_assignment_at = new_package_sender_info[:last_empty_package_to_ship_assigned_at]
      reset_weekly_assignment_count = last_assignment_at &&
        (consignment_week_number(last_assignment_at) != consignment_week_number(Time.now))
      services.consignment_service.assign_package_to_ship_request_to_package_sender(
        new_package_sender[:id], reset_weekly_assignment_count
      )
      services.consignment_service.add_request_to_package_sender_bucket(
        new_package_sender[:id], consignment_request[:id]
      )
      consignment_request = services.consignment_service.update_package_sender_by_admin(
        consignment_request_id, new_package_sender[:id], access_context[:access_token].identity
      )
      services.user_service.increment_consignment_package_sender_requests_count(new_package_sender[:id])

      consignment_package_request_reassign_comms(consignment_request, old_package_sender_id, previous_state)
    end

    def supplier_update_consignment_request(access_context, supplier_id, consignment_request_id, consignment_request_data)
      supplier_id = check_access(access_context[:access_token], supplier_id)

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      unless supplier_id == consignment_request[:consignment_supplier_id]
        raise GoshPosh::Platform::Errors::InvalidConsignmentRequestError.new(
          GoshPosh::Platform::Errors::ValidationErrorMessages::UNABLE_TO_PROCESS_REQUEST_CONTACT_SUPPORT
        )
      end

      if consignment_request_data[:consignment_supplier_address_id]
        address = get_consignment_request_address(
          supplier_id, consignment_request_data[:consignment_supplier_address_id]
        )

        unless partner_available_nearby?(access_context, supplier_id, address)
          raise GoshPosh::Platform::Errors::InvalidConsignmentRequestAddressError
        end

        calculate_address_coordinates(supplier_id, address, update_address_book: true)
        services.consignment_service.update_consignment_request_on_address_change(
          consignment_request_id,
          address,
          GoshPosh::Platform::Consignments::ConsignmentRequestState::SCHEDULING_PICKUP_ALLOWED_STATES + [
            GoshPosh::Platform::Consignments::ConsignmentRequestState::NEW
          ],
          consignment_request[:notify_capacity_availability]
        )
        update_consignment_request_index(consignment_request_id)
      end

      if consignment_request_data[:notify_capacity_availability]
        services.consignment_service.update_notify_capacity_availability_for_consignment_request(
          consignment_request_id, consignment_request_data[:notify_capacity_availability]
        )
        update_consignment_request_index(consignment_request_id)
      end

      shipment_collection_info = {}
      if consignment_request_data[:pickup_start_at] && consignment_request_data[:pickup_end_at]
        shipment_collection_info[:pickup_start_at] = Time.parse(consignment_request_data[:pickup_start_at])
        shipment_collection_info[:pickup_end_at] = Time.parse(consignment_request_data[:pickup_end_at])
      end

      if consignment_request_data[:pickup_notes]
        shipment_collection_info[:pickup_notes] = consignment_request_data[:pickup_notes]
      end

      return if shipment_collection_info.empty?

      consignment_supplier_update_pickup_info(access_context, consignment_request, shipment_collection_info)
      update_consignment_request_index(consignment_request_id)
    end

    def add_consignment_note_as_admin(access_context, consignment_request_id, note)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])
      admin_id = access_context[:access_token].identity
      services.consignment_service.add_note_to_consignment_request(consignment_request_id, note, admin_id)
      update_consignment_request_index(consignment_request_id)
    end

    def update_consignment_request_state_as_admin(access_context, consignment_request_id, state, reason: nil, reason_note: nil)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::UPDATE_CONSIGNMENT_STATE])

      state = state.to_sym
      consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      )

      case state
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SHIPPED_TO_SUPPLIER
        consignment_request_machine.package_shipped_to_supplier(access_context)
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_DELIVERED_TO_SUPPLIER
        consignment_request_machine.package_delivered_to_supplier(access_context)
        consignment_request_machine.await_pickup_schedule(access_context)
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SHIPPED_TO_PARTNER
        consignment_request_machine.package_shipped_to_partner(access_context)
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::PENDING_INVENTORY_PROCESSING
        consignment_request_machine.package_delivered_to_partner(access_context)
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::INVENTORY_IN_PROCESS
        consignment_request_machine.reopen_inventory_processing(access_context)
      when GoshPosh::Platform::Consignments::ConsignmentRequestState::CANCELLED
        unless GoshPosh::Platform::Consignments::ConsignmentRequestCancelReason::ADMIN_REASONS.include?(reason)
          raise GoshPosh::Platform::Errors::InvalidInputError.new("Cancel reason #{reason} is invalid ")
        end

        if consignment_request_machine.consignment_request[:aggregates][:total_items].positive?
          raise GoshPosh::Platform::Errors::InvalidInputError.new(
            'Cannot cancel consignment request with active listings'
          )
        end

        consignment_request_machine.cancel_consignment_request(
          access_context,
          GoshPosh::Platform::Consignments::ConsignmentRequestState::ADMIN_CANCEL_ALLOWED_STATES,
          reason,
          reason_note
        )
      else
        raise GoshPosh::Platform::Errors::InvalidInputError
      end
    end

    def cancel_and_recreate_consignment_request_as_admin(access_context, consignment_request_id, reason: nil, reason_note: nil)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      unless GoshPosh::Platform::Consignments::ConsignmentRequestCancelReason::ADMIN_REASONS.include?(reason)
        raise GoshPosh::Platform::Errors::InvalidInputError.new("Cancel reason #{reason} is invalid ")
      end

      consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      )

      if consignment_request_machine.consignment_request[:aggregates][:total_items].positive?
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          'Cannot cancel consignment request with active listings'
        )
      end

      consignment_request_machine.cancel_consignment_request(
        access_context,
        GoshPosh::Platform::Consignments::ConsignmentRequestState::ADMIN_CANCEL_ALLOWED_STATES,
        reason,
        reason_note
      )

      cancelled_consignment_request = consignment_request_machine.consignment_request

      begin
        recreate_consignment_request_for_quick_matching(
          access_context,
          cancelled_consignment_request,
          attempt_hard_match: false
        )
      rescue => error
        services.logger.error(
          GoshPosh::Platform::Util.print_stack_trace(
            "#{__method__} Failed to recreate consignment_request: #{consignment_request_id}",
            error
          )
        )
        raise error
      end
    end

    def initiate_consignment_chat(access_context, user_id, consignment_request_id, chat_params)
      check_access(access_context[:access_token], user_id)

      consignment_chat_support_request_id = services.consignment_service.latest_consignment_chat_support_request_id(
        consignment_request_id
      )

      if consignment_chat_support_request_id
        user_add_message_to_consignment_chat_support_request(
          access_context,
          user_id,
          consignment_request_id,
          consignment_chat_support_request_id,
          chat_params
        )
      else
        create_consignment_chat_support_request(access_context, user_id, consignment_request_id, chat_params)
      end
    end

    def create_consignment_chat_support_request(access_context, user_id, consignment_request_id, chat_params)
      user_id = check_access(access_context[:access_token], user_id)

      enforce_rate_limits_v2(
        :rate_limits,
        :add_message_to_support_request,
        "th|u:{#{user_id}}|a:msg:sr", # common throttle for all support request comments
        GoshPosh::Settings.throttles[:add_message_to_support_request][:rate_limits],
        true
      ) do
        consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
        raise consignment_chat_unable_to_process_error unless consignment_request

        actor = services.user_service.by_id(user_id)
        validate_consignment_chat_message_params(access_context, actor, chat_params, consignment_request)
        validate_user_adding_consignment_chat_message(actor, consignment_request)

        sr_actor_type = consignment_sr_actor_type_by_user_id(access_context, consignment_request, user_id)
        support_request_params = {
          type: GoshPosh::Platform::Commerce::SupportRequestType::CONSIGNMENT_REQUEST_CHAT,
          consignment_request_id: consignment_request_id,
          actor: sr_actor_type,
          data: { sr_label: GoshPosh::Platform::Commerce::SupportRequestLabel::CONSIGNMENT_REQUEST_CHAT },
          message: chat_params[:user_message],
          pictures: chat_params[:pictures]
        }
        chat_support_request = services.order_service.create_support_request(
          actor,
          GoshPosh::Platform::Commerce::SupportRequestActionRequested::CONSIGNMENT_REQUEST_CHAT,
          support_request_params
        )

        services.consignment_service.add_consignment_support_request_lookup(chat_support_request)
        services.consignment_service.consignment_request_chat_message_added(consignment_request[:id], sr_actor_type)
        update_consignment_request_index(consignment_request[:id])
        chat_params_event_logger = {
          message: chat_params[:user_message],
          actor: sr_actor_type
        }
        services.event_logger_v2.send_user_message_to_ind_or_consignment_chat(
          access_context,
          actor,
          consignment_request,
          chat_params_event_logger,
          true,
          chat_support_request[:id],
          GoshPosh::Platform::Commerce::SupportRequestType::CONSIGNMENT_REQUEST_CHAT
        )

        recipient_sr_actor_type = sr_actor_counter_party_type_by_type(sr_actor_type)
        recipient_id = consignment_user_id_from_sr_actor_type(consignment_request, recipient_sr_actor_type)
        user_commented_on_consignment_chat_comms(
          consignment_request,
          actor_id: user_id,
          recipient_id: recipient_id,
          recipient_sr_actor_type: recipient_sr_actor_type
        )

        chat_support_request
      end
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::USER_INITIATE_CONSIGNMENT_CHAT_ERROR,
        __FILE__,
        __method__,
        { consignment_request_id: consignment_request_id, user_id: user_id }
      )

      raise
    end

    def user_add_message_to_consignment_chat_support_request(
      access_context,
      user_id,
      consignment_request_id,
      sr_id,
      chat_params
    )
      user_id = check_access(access_context[:access_token], user_id)

      enforce_rate_limits_v2(
        :rate_limits,
        :add_message_to_support_request,
        "th|u:{#{user_id}}|a:msg:sr", # common throttle for all support request comments
        GoshPosh::Settings.throttles[:add_message_to_support_request][:rate_limits],
        true
      ) do
        user = services.user_service.by_id(user_id)

        consignment_request, support_request = validate_consignment_request_and_latest_support_request_link(
          consignment_request_id, sr_id
        )

        validate_consignment_chat_message_params(access_context, user, chat_params, consignment_request)
        validate_user_adding_consignment_chat_message(user, consignment_request)

        sr_actor_type = consignment_sr_actor_type_by_user_id(access_context, consignment_request, user_id)
        updated_support_request = services.order_service.add_user_message_to_consignment_support_request(
          sr_id: support_request[:id],
          consignment_request_id: consignment_request[:id],
          actor_id: user_id,
          actor: sr_actor_type,
          message: chat_params[:user_message],
          pictures: chat_params[:pictures]
        )
        raise consignment_chat_unable_to_process_error unless updated_support_request

        services.consignment_service.consignment_request_chat_message_added(consignment_request[:id], sr_actor_type)
        update_consignment_request_index(consignment_request[:id])
        reopen_temporarily_resolved_support_case(access_context, consignment_request, support_request)
        chat_params_event_logger = {
          message: chat_params[:user_message],
          actor: sr_actor_type
        }
        services.event_logger_v2.send_user_message_to_ind_or_consignment_chat(
          access_context,
          user,
          consignment_request,
          chat_params_event_logger,
          false,
          support_request[:id],
          GoshPosh::Platform::Commerce::SupportRequestType::CONSIGNMENT_REQUEST_CHAT
        )

        recipient_sr_actor_type = sr_actor_counter_party_type_by_type(sr_actor_type)
        recipient_id = consignment_user_id_from_sr_actor_type(consignment_request, recipient_sr_actor_type)
        user_commented_on_consignment_chat_comms(
          consignment_request,
          actor_id: user_id,
          recipient_id: recipient_id,
          recipient_sr_actor_type: recipient_sr_actor_type
        )

        updated_support_request
      end
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::USER_ADD_CONSIGNMENT_CHAT_MESSAGE_ERROR,
        __FILE__,
        __method__,
        { user_id: user_id, sr_id: sr_id, consignment_request_id: consignment_request_id }
      )

      raise
    end


    def consignment_support_request_interactions(access_context, user_id, consignment_request_id)
      check_access(access_context[:access_token], user_id)

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      unless consignment_request
        raise consignment_chat_unable_to_process_error
      end

      unless GoshPosh::Platform::Consignments::ConsignmentRequest.valid_supplier_or_partner?(consignment_request, user_id)
        raise consignment_chat_unable_to_process_error
      end

      support_requests = services.consignment_service.support_requests_by_consignment_request_id(
        consignment_request_id,
        sr_type: GoshPosh::Platform::Commerce::SupportRequestType::CONSIGNMENT_REQUEST_CHAT
      )
      support_request_interactions = []
      support_requests.each do |support_request|
        support_request_interactions.concat(
          consignment_support_chat_interactions(access_context, user_id, consignment_request, support_request)
        )
      end

      # oldest first
      support_request_interactions =
        support_request_interactions.sort { |x, y| [y[:timestamp]] <=> [x[:timestamp]] }.reverse

      presentation_info = {
        header_message_enabled: consignment_chat_header_visible?(
          user_id, consignment_request, support_request_interactions
        ),
        tooltip_enabled: true,
        chat_enabled: consignment_chat_enabled?(user_id, consignment_request)
      }

      {
        data: support_request_interactions,
        presentation: presentation_info
      }
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::USER_VIEW_CONSIGNMENT_CHAT_ERROR,
        __FILE__,
        __method__,
        { user_id: user_id, consignment_request_id: consignment_request_id }
      )
      raise
    end

    def partner_available_nearby?(access_context, user_id, address, radius_type: :default)
      address = calculate_address_coordinates(user_id, address, update_address_book: false)
      user_home_domain = get_user_home_domain(access_context, user_id)
      consignment_schedule_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_SCHEDULE,
        user_home_domain
      )

      case radius_type
      when :default
        search_radius = consignment_schedule_fs[:supplier_enrollment_partner_search_radius_miles]
      when :extended
        search_radius = consignment_schedule_fs[:supplier_enrollment_extended_partner_search_radius_miles]
      when :uber_default
        search_radius = consignment_schedule_fs[:uber_default_partner_search_radius_miles]
      else
        raise GoshPosh::Platform::Errors::InvalidInputError.new('Invalid radius type')
      end

      float_coordinates = {
        latitude: address[:coordinates][:latitude].to_f,
        longitude: address[:coordinates][:longitude].to_f
      }
      nearby_partner_ids = find_consignment_partners_for_pickup(
        pickup_coordinates: float_coordinates,
        partner_search_radius_miles: search_radius,
        minimum_partner_capacity: 0,
        id_only: true,
        limit: 2 # accounting for when caller is a partner
      )

      nearby_partner_ids.reject! { |partner_id| partner_id.to_s == user_id.to_s } if user_id

      nearby_partner_ids.any?
    end

    def get_consignment_partner_request_schedules(access_context, user_id)
      partner_id = check_access(access_context[:access_token], user_id)

      consignment_schedule_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_SCHEDULE,
        access_context[:home_domain]
      )
      unless consignment_schedule_fs && consignment_schedule_fs[:enabled]
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::CONSIGNMENT_SCHEDULE_NOT_ENABLED
        )
      end

      partner_opt_in_allowed = !consignment_wind_down_enabled?(access_context, partner_id)

      partner_info = services.consignment_service.consignment_partner_info(partner_id)
      unless partner_info
        raise GoshPosh::Platform::Errors::ConsignmentError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::CONSIGNMENT_ERROR_GENERIC_TRY_AGAIN
        )
      end

      visible_schedule_weeks_count = consignment_schedule_fs[:partner_visible_schedule_weeks_count]

      unless GoshPosh::Platform::Consignments::ConsignmentPartnerState::ENABLED_STATES.include?(partner_info&.dig(:state)) &&
             visible_schedule_weeks_count.positive?
        return { data: [], meta: {} }
      end

      current_time = Time.now
      current_week_number = consignment_week_number(current_time)
      ending_week_number = consignment_week_number(current_time + visible_schedule_weeks_count.weeks)
      beginning_week_start_at = week_number_to_consignment_weekly_time_window(current_week_number).first
      ending_week_end_at = week_number_to_consignment_weekly_time_window(ending_week_number).last

      consignment_requests = get_partner_matched_and_scheduled_requests(
        partner_id,
        pickup_start_at: beginning_week_start_at,
        pickup_end_at: ending_week_end_at
      )

      supplier_weekly_references = {}
      supplier_references = consignment_user_references(consignment_requests, [:consignment_supplier_id])

      consignment_requests.each do |request|
        pickup_week_number = consignment_week_number(
          request[:supplier_shipment_collection_info][:pickup_start_at]
        )

        supplier_weekly_references[pickup_week_number] ||= []
        supplier_weekly_references[pickup_week_number] << supplier_references[request[:consignment_supplier_id]]
      end

      partner_weekly_schedules = []
      weekly_actions = []
      visible_schedule_weeks_count.times do |week_offset|
        week_number = consignment_week_number(current_time + week_offset.weeks)
        week_start_at, week_end_at = week_number_to_consignment_weekly_time_window(week_number)
        weekly_partner_capacity_info = effective_partner_weekly_capacity_info(
          partner_id, partner_info, week_number, week_start_at
        )

        current_week_schedules = {
          week_number: week_number,
          week_start_at: week_start_at.iso8601,
          week_end_at: week_end_at.iso8601,
          state: weekly_partner_capacity_info[:state]
        }

        if (weekly_partner_capacity_info[:state] == GoshPosh::Platform::Consignments::ConsignmentPartnerCapacityState::ENABLED) &&
           weekly_partner_capacity_info[:total_capacity].positive?
          current_week_schedules[:consignment_supplier_references] = supplier_weekly_references[week_number] || []
          unused_partner_schedule_capacity_count(partner_info, weekly_partner_capacity_info).times do
            current_week_schedules[:consignment_supplier_references] << dummy_consignment_supplier_reference_object
          end
        else
          current_week_schedules[:message] = 'Not accepting bags'
          current_week_schedules[:state] =
            GoshPosh::Platform::Consignments::ConsignmentPartnerCapacityState::DISABLED.to_s
        end

        partner_weekly_schedules << current_week_schedules

        allowed_partner_capacity_actions = build_allowed_partner_capacity_actions(
          weekly_partner_capacity_info,
          consignment_schedule_fs[:secure_matching],
          partner_opt_in_allowed: partner_opt_in_allowed
        )
        if allowed_partner_capacity_actions.any?
          weekly_actions << { week_number: week_number, actions: allowed_partner_capacity_actions }
        end
      end

      {
        data: partner_weekly_schedules,
        meta: { actions: weekly_actions }
      }
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::GET_CONSIGNMENT_PARTNER_REQUEST_SCHEDULED_ERROR,
        __FILE__,
        __method__,
        { user_id: user_id }
      )
      raise error
    end

    def get_consignment_partner_enrollment_requests(access_context, user_id)
      check_access(access_context[:access_token], user_id)

      user = services.user_service.by_id(user_id)
      if user[:consignment_supplier]
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::EXISTING_SUPPLIER_ERROR
        )
      end

      enrollment_requests = services.consignment_service.get_consignment_partner_enrollment_requests(
        user_id, GoshPosh::Platform::Consignments::ConsignmentPartnerEnrollmentState::ACTIVE_AND_APPROVED_PARTNER_ENROLLMENT_STATES
      )

      if enrollment_requests&.any?
        address = services.order_service.default_consignment_address_in_user_address_list(user_id)
        validate_address_po_box(address, is_consignment: true)
        unless address
          raise GoshPosh::Platform::Errors::AddressNotFoundError.new(
            GoshPosh::Platform::Errors::ConsignmentErrorMessages::CONSIGNMENT_ERROR_GENERIC_TRY_AGAIN
          )
        end

        data = enrollment_requests.collect do |enrollment_request|
          enrollment_request.slice(*PARTNER_ENROLLMENT_FIELDS)
        end

        presentation = {}
        unless user[:consignment_partner]
          presentation[:enrollment_banner] = {
            title: PARTNER_ENROLLMENT_SUBMITTED_BANNER_TITLE,
            message: PARTNER_ENROLLMENT_SUBMITTED_BANNER_MESSAGE
          }
        end

        return { data: data, meta: {}, presentation: presentation }
      end

      address = nil
      begin
        address = get_consignment_request_address(user_id)
      rescue => error
        services.logger.warn GoshPosh::Platform::Util.print_stack_trace(
          "Error in #{__method__} user_id: #{user_id}", error
        )
      end


      user_address_list = services.order_service.get_user_address_list(user_id)
      if address && user_address_list[:default_consignment_address_id] != address[:id]
        validate_address_po_box(address, is_consignment: true)
        services.order_service.set_default_address_in_user_address_list(
          user_id, [GoshPosh::Platform::Commerce::DefaultAddress::CONSIGNMENT], address[:id]
        )
      end

      meta = { consignment_partner_address: address }
      services.user_service.get_phone_number_id_for_user(user_id)&.tap do |phone_number_id|
        meta[:phone_number_id] = phone_number_id
        meta[:phone_number] = services.user_service.get_masked_phone_number_for_id(phone_number_id)
      end

      { data: [], meta: meta, presentation: {} }
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::GET_CONSIGNMENT_PARTNER_ENROLLMENT_REQUEST_ERROR,
        __FILE__,
        __method__,
        { user_id: user_id }
      )
      raise error
    end

    def get_consignment_partner_settings(access_context, user_id)
      check_access(access_context[:access_token], user_id)

      consignment_partner_info = services.consignment_service.consignment_partner_info(user_id)

      unless consignment_partner_info
        raise GoshPosh::Platform::Errors::ConsignmentError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::CONSIGNMENT_ERROR_GENERIC_TRY_AGAIN
        )
      end

      state = consignment_partner_info[:state]
      unless state == GoshPosh::Platform::Consignments::ConsignmentPartnerState::DISABLED
        state = GoshPosh::Platform::Consignments::ConsignmentPartnerState::ENABLED
      end
      capacity = consignment_partner_info[:capacity]
      package_frequency = consignment_partner_info[:package_frequency]&.to_s

      package_frequency_display_label =
        if package_frequency&.to_sym == GoshPosh::Platform::Consignments::ConsignmentPartnerPackageFrequency::BIWEEKLY
          "#{capacity} Bag#{'s' if capacity.to_i > 1} Every 2 Weeks"
        else
          "#{capacity} Bag#{'s' if capacity.to_i > 1} Weekly"
        end

      {
        data: {
          state: state,
          capacity: capacity,
          package_frequency: package_frequency
        },
        presentation: {
          package_frequency_display_label: package_frequency_display_label
        }
      }
    end

    def update_consignment_partner_settings(access_context, partner_id, params)
      check_access(access_context[:access_token], partner_id)

      partner = services.user_service.by_id(partner_id)
      if partner[:consignment_supplier] || !partner[:consignment_partner]
        raise consignment_invalid_input_try_again_error
      end

      # Get consignment feature settings
      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        access_context[:home_domain]
      )

      # Handle state parameter if provided and feature is enabled
      if params[:state] && consignment_fs&.dig(:allow_partner_self_pause)

        unless params[:state].to_sym == GoshPosh::Platform::Consignments::ConsignmentPartnerState::DISABLED
          raise consignment_invalid_input_try_again_error
        end

        # Update the consignment partner state
        services.consignment_service.update_consignment_partner_state(
          partner_id,
          { state: params[:state].to_sym },
          access_context[:access_token].identity
        )

        # Log the state change event
        begin
          services.event_logger_v2.consignment_partner_update_settings(
            access_context,
            { consignment_partner_state: params[:state].to_sym },
            partner
          )
        rescue => log_error
          services.logger.error GoshPosh::Platform::Util.print_stack_trace(
            "Error in #{__method__} while logging event partner_id: #{partner_id}", log_error
          )
        end

        begin
          send_consignment_partner_self_paused_future_packages_mail(partner)
          abandon_consignment_requests_when_partner_is_disabled(partner[:id])
        rescue => comms_error
          services.logger.error GoshPosh::Platform::Util.print_stack_trace(
            "Error in #{__method__} partner_id: #{partner_id}", comms_error
          )
        end
      elsif params[:capacity] || params[:package_frequency]
        capacity = params[:capacity]&.to_i || 0
        package_frequency = params[:package_frequency]&.to_sym

        unless valid_consignment_partner_capacity_frequency_info?(capacity, package_frequency, access_context[:home_domain])
          raise consignment_invalid_input_try_again_error
        end

        settings = {
          capacity: capacity,
          package_frequency: package_frequency
        }
        updated_settings = services.consignment_service.upsert_consignment_partner_info(
          partner_id, settings, actor_id: access_context[:access_token].identity
        )

        update_partner_weekly_capacities_on_capacity_settings_changes(access_context, partner_id, updated_settings)

        begin
          services.event_logger_v2.consignment_partner_update_settings(
            access_context,
            { capacity: capacity, package_frequency: package_frequency },
            partner
          )
        rescue => log_error
          services.logger.error GoshPosh::Platform::Util.print_stack_trace(
            "Error in #{__method__} while logging event partner_id: #{partner_id}", log_error
          )
        end
      end

    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::UPDATE_CONSIGNMENT_PARTNER_SETTINGS_ERROR,
        __FILE__,
        __method__,
        { user_id: partner_id }
      )
      raise error
    end

    def admin_get_consignment_partner_enrollment_requests(access_context, page = 1, count = 50)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      consignment_partner_enrollment_requests = services.consignment_service.get_all_consignment_partner_enrollment_requests(page, count)

      {
        consignment_partner_enrollment_requests: consignment_partner_enrollment_requests
      }
    end

    def get_consignment_supplier_enrollment_requests(access_context, user_id)
      check_access(access_context[:access_token], user_id)
      auth_session_id = (access_context && access_context[:access_token] && access_context[:access_token].guest ? access_context[:access_token].auth_session_id : nil)

      user = services.user_service.by_id(user_id)
      if user[:consignment_partner]
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::ERROR_GENERIC
        )
      end

      if consignment_wind_down_enabled?(access_context, user_id)
        return closed_supplier_enrollment_response(user[:home_domain])
      end

      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        user[:home_domain]
      )

      supplier_enrollment_requests = services.consignment_service.get_consignment_supplier_enrollment_requests(
        user_id, GoshPosh::Platform::Consignments::ConsignmentSupplierEnrollmentState::ACTIVE_AND_APPROVED_SUPPLIER_ENROLLMENT_STATES
      )
      if auth_session_id && !supplier_enrollment_requests.empty?
        supplier_enrollment_requests.delete_if { |request| request[:auth_session_id] != auth_session_id }
      end
      if supplier_enrollment_requests.empty? && user[:guest_user_id].present?
        # We don't migrate guest user enrollment data
        supplier_enrollment_requests = services.consignment_service.get_consignment_supplier_enrollment_requests(
          user[:guest_user_id], GoshPosh::Platform::Consignments::ConsignmentSupplierEnrollmentState::ACTIVE_AND_APPROVED_SUPPLIER_ENROLLMENT_STATES
        )
      end

      is_old_app_version = GoshPosh::Platform::Util.is_app_version_between_range(
        access_context[:app_version], nil, GoshPosh::Settings.consignment_onboarding_label_max_version# 9.02
      )

      auto_approve_fs_enabled = consignment_fs[:auto_approve_enrollment_request]

      # user already enrolled
      if supplier_enrollment_requests && !supplier_enrollment_requests.empty?
        address = services.order_service.default_consignment_address_in_user_address_list(user_id, auth_session_id)
        unless address
          raise GoshPosh::Platform::Errors::AddressNotFoundError.new(
            GoshPosh::Platform::Errors::ConsignmentErrorMessages::ERROR_GENERIC
          )
        end

        data = []
        supplier_enrollment_requests.each do |enrollment_request|
          enrollment_request_data = enrollment_request.slice(*SUPPLIER_ENROLLMENT_FIELDS)
          enrollment_request_data[:consignment_supplier_address] = address
          data << enrollment_request_data
        end

        if user[:consignment_supplier]
          meta = {
            consignment_supplier_address: address,
            consignment_supplier_address_serviceable: partner_available_nearby?(access_context, user_id, address)
          }
          presentation = nil
          if is_old_app_version
            presentation = supplier_enrollment_requests_presentation(access_context, user_id, address)
          end
        else
          meta = nil
          presentation = supplier_enrollment_requests_presentation(access_context, user_id, address)
        end

        return {
          data: data,
          meta: meta,
          presentation: presentation
        }
      end

      begin
        address = get_consignment_request_address(user_id, nil, auth_session_id)
      rescue => error
        services.logger.warn GoshPosh::Platform::Util.print_stack_trace(
          "Error in #{__method__} user_id: #{user_id}", error
        )
        if consignment_wind_down_enabled?(access_context, user_id)
          return closed_supplier_enrollment_response(user[:home_domain])
        end

        return { presentation: { action: { label: GET_STARTED_LABEL } } }
      end


      user_address_list = services.order_service.get_user_address_list(user_id)
      if user_address_list[:default_consignment_address_id] != address[:id]
        services.order_service.set_default_address_in_user_address_list(
          user_id, [GoshPosh::Platform::Commerce::DefaultAddress::CONSIGNMENT], address[:id]
        )
      end

      nearby_partners_available = false
      extended_partners_available = false

      begin
        nearby_partners_available = partner_available_nearby?(access_context, user_id, address)
      rescue => error
        services.logger.warn GoshPosh::Platform::Util.print_stack_trace(
          "Error in #{__method__} user_id: #{user_id}", error
        )
        return { presentation: { action: { label: NOTIFY_WHEN_AVAILABLE_LABEL } } }
      end

      unless nearby_partners_available
        extended_partners_available = partner_available_nearby?(access_context, user_id, address, radius_type: :extended)
      end

      if nearby_partners_available
        disclaimer = (is_old_app_version || !auto_approve_fs_enabled) ? CONSIGNMENT_LAUNCHED_MESSAGE : nil
        label = (is_old_app_version || !auto_approve_fs_enabled) ? NOTIFY_WHEN_AVAILABLE_LABEL : GET_STARTED_LABEL
      elsif extended_partners_available
        disclaimer = CONSIGNMENT_LAUNCHED_MESSAGE
        label = NOTIFY_WHEN_AVAILABLE_LABEL
      else
        disclaimer = CONSIGNMENT_LAUNCHING_SOON_MESSAGE
        label = NOTIFY_WHEN_AVAILABLE_LABEL
      end


      {
        meta: {
          consignment_supplier_address: address,
          consignment_supplier_address_serviceable: auto_approve_fs_enabled ? nearby_partners_available : false
        },
        presentation: {
          service_availability_disclaimer: {
            text: disclaimer
          },
          action: {
            label: label
          }
        }
      }
    rescue => error
      services.logger.error GoshPosh::Platform::Util.print_stack_trace(
        "Error in #{__method__} user_id: #{user_id}", error
      )
      raise error
    end

    def create_consignment_partner_enrollment_request(access_context, user_id, address_notes)
      check_access(access_context[:access_token], user_id)

      user = services.user_service.by_id(user_id)
      validate_program_accepting_new_enrollment!(access_context, user)

      if user[:consignment_supplier]
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::EXISTING_SUPPLIER_ERROR
        )
      elsif user[:consignment_partner]
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::CONSIGNMENT_ERROR_GENERIC_TRY_AGAIN
        )
      end

      enrollment_requests = services.consignment_service.get_consignment_partner_enrollment_requests(
        user_id, GoshPosh::Platform::Consignments::ConsignmentPartnerEnrollmentState::ACTIVE_PARTNER_ENROLLMENT_STATES
      )
      raise GoshPosh::Platform::Errors::InvalidInputError.new(
        GoshPosh::Platform::Errors::ConsignmentErrorMessages::CONSIGNMENT_ERROR_GENERIC_TRY_AGAIN
      ) if enrollment_requests&.any?

      if address_notes
        validate_consignment_address_notes(user, address_notes, actor: GoshPosh::Platform::ConsignmentActor::PARTNER)
      end

      address = get_consignment_request_address(user_id)

      unless services.user_service.get_unmasked_phone_number_for_user(user_id)
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::CONSIGNMENT_ERROR_GENERIC_TRY_AGAIN
        )
      end

      enrollment_request = services.consignment_service.create_consignment_partner_enrollment_request(user_id, address_notes)

      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        user[:home_domain]
      )

      if consignment_fs[:auto_approve_partner_enrollment_request]
        make_user_a_consignment_partner(access_context, user_id, GoshPosh::Platform::POSHMARK_ID, enrollment_request)
        GoshPosh::Runtime::PmLogger.instance.consignment_event(
          consignment_event: :auto_approve_partner_enrollment_request,
          data: { user_id: user_id }
        )
        services.event_logger_v2.submit_enrollment_request(
          access_context,
          user,
          { status: GoshPosh::Platform::Consignments::ConsignmentPartnerEnrollmentState::APPROVED },
          'consignment_partner',
          address,
          nil
        )
      else
        cancel_active_supplier_enrollment_request(user_id, GoshPosh::Platform::POSHMARK_ID)
        update_user_tags(
          user_id,
          GoshPosh::Platform::Users::Tag.construct_tags(
            tags_to_insert: { pcp_approval_pending: true },
            tags_to_remove: { pcs_eligible: false, pcs_approval_pending: true }
          )
        )

        services.event_logger_v2.submit_enrollment_request(
          access_context,
          user,
          { status: GoshPosh::Platform::Consignments::ConsignmentPartnerEnrollmentState::SUBMITTED },
          'consignment_partner',
          address,
          nil
        )
      end

      {}
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::CREATE_CONSIGNMENT_PARTNER_ENROLLMENT_REQUEST_ERROR,
        __FILE__,
        __method__,
        { user_id: user_id }
      )
      raise error
    end

    def create_consignment_supplier_enrollment_request(access_context, user_id)
      check_access(access_context[:access_token], user_id)
      auth_session_id = (access_context && access_context[:access_token] && access_context[:access_token].guest ? access_context[:access_token].auth_session_id : nil)

      user = services.user_service.by_id(user_id)
      validate_program_accepting_new_enrollment!(access_context, user)

      if user[:consignment_partner] || user[:consignment_supplier]
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::ERROR_GENERIC
        )
      end

      supplier_enrollment_request = services.consignment_service.get_consignment_supplier_enrollment_requests(
        user_id, GoshPosh::Platform::Consignments::ConsignmentSupplierEnrollmentState::ACTIVE_SUPPLIER_ENROLLMENT_STATES
      )

      if supplier_enrollment_request && !supplier_enrollment_request.empty?
        raise GoshPosh::Platform::Errors::InvalidInputError.new(GoshPosh::Platform::Errors::ConsignmentErrorMessages::ERROR_GENERIC)
      end

      enrollment_request = services.consignment_service.create_consignment_supplier_enrollment_request(user_id, auth_session_id)
      address = get_consignment_request_address(user_id, nil, auth_session_id)
      if address
        consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
          GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
          user[:home_domain]
        )

        partner_available_nearby = false
        if consignment_fs[:auto_approve_enrollment_request]
          partner_available_nearby = partner_available_nearby?(access_context, user_id, address)
        end
        partner_within_uber_serviceable_radius = partner_available_nearby?(
          access_context, user_id, address, radius_type: :uber_default
        )

        if partner_available_nearby
          add_user_consignment_supplier_tag(user_id, GoshPosh::Platform::POSHMARK_ID)
          GoshPosh::Runtime::PmLogger.instance.consignment_event(
            consignment_event: :auto_approve_supplier_enrollment_request,
            data: { user_id: user_id, guest: user[:guest] }
          )
          services.event_logger_v2.submit_enrollment_request(
            access_context,
            user,
            { status: GoshPosh::Platform::Consignments::ConsignmentSupplierEnrollmentState::APPROVED },
            'consignment_supplier',
            address,
            true,
            partner_within_uber_serviceable_radius
          )
        end

        unless partner_available_nearby
          cancel_active_partner_enrollment_request(user_id, GoshPosh::Platform::POSHMARK_ID)
          update_user_tags(
            user_id,
            GoshPosh::Platform::Users::Tag.construct_tags(
              tags_to_insert: { pcs_approval_pending: true },
              tags_to_remove: { pcp_approval_pending: false }
            )
          )

          in_serviceable_area = partner_available_nearby?(access_context, user_id, address, radius_type: :extended)
          services.event_logger_v2.submit_enrollment_request(
            access_context,
            user,
            { status: GoshPosh::Platform::Consignments::ConsignmentSupplierEnrollmentState::SUBMITTED },
            'consignment_supplier',
            address,
            in_serviceable_area,
            partner_within_uber_serviceable_radius
          )
        end
      end

      enrollment_request = services.consignment_service.get_consignment_supplier_enrollment_request_by_id(enrollment_request[:id])

      { data: enrollment_request.slice(*SUPPLIER_ENROLLMENT_FIELDS) }
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::CREATE_CONSIGNMENT_SUPPLIER_ENROLLMENT_REQUEST_ERROR,
        __FILE__,
        __method__,
        { user_id: user_id }
      )
      raise error
    end

    def generate_new_consignment_shipping_label(access_context, user_id, consignment_request_id, reason, shipping_label_type)
      check_access(access_context[:access_token], user_id)

      raise GoshPosh::Platform::Errors::InvalidConsignmentLabelGenerationRequestError if reason.nil?

      consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      )
      consignment_request = consignment_request_machine.consignment_request

      case reason.to_sym
      when GoshPosh::Platform::Commerce::ShippingLabelReason::CONSIGNMENT_PACKAGE_SENDER_TO_PCS
        unless consignment_request[:consignment_package_sender_id] == user_id
          raise GoshPosh::Platform::Errors::InvalidConsignmentLabelGenerationRequestError
        end

        if consignment_request[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SENDER_SHIPMENT_LABEL_GENERATION_IN_PROGRESS
          raise GoshPosh::Platform::Errors::InvalidConsignmentLabelGenerationRequestError.new(
            GoshPosh::Platform::Errors::ConsignmentErrorMessages::SHIPPING_LABEL_GENERATION_IN_PROGRESS
          )
        elsif !GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SENDER_LABEL_GENERATION_ALLOWED_STATES.include?(consignment_request[:state])
          raise GoshPosh::Platform::Errors::InvalidConsignmentLabelGenerationRequestError
        end
        home_domain = get_user_home_domain(access_context, user_id)
        sls_fs = GoshPosh::FeatureSettings.get_domain_based('sls_scam', home_domain)
        if sls_fs && sls_fs[:enabled]
          package_sender_limit = sls_fs[:max_package_sender_labels_per_consignment_limit]
          requested_labels = services.shipping_service.get_shipping_labels_of_consignment_request(
            consignment_request_id,
            [GoshPosh::Platform::Commerce::ShippingLabelState::ACTIVE, GoshPosh::Platform::Commerce::ShippingLabelState::USED],
            [GoshPosh::Platform::Commerce::ShippingLabelReason::CONSIGNMENT_PACKAGE_SENDER_TO_PCS]
          )

          if package_sender_limit && requested_labels.size >= package_sender_limit
            raise GoshPosh::Platform::Errors::ShippingLabelError.new(
              GoshPosh::Platform::Errors::ConsignmentErrorMessages::SHIPPING_LABEL_LIMIT_EXCEEDED
            )
          end
        end

        latest_label = services.shipping_service.get_latest_consignment_shipping_label(
          consignment_request[:id],
          nil,
          [ GoshPosh::Platform::Commerce::ShippingLabelState::ACTIVE,
            GoshPosh::Platform::Commerce::ShippingLabelState::USED ],
          [ GoshPosh::Platform::Commerce::ShippingLabelReason::CONSIGNMENT_PACKAGE_SENDER_TO_PCS ]
        )

        if latest_label && latest_label[:shipping_label_type] == shipping_label_type.to_sym &&
          latest_label[:state] == GoshPosh::Platform::Commerce::ShippingLabelState::ACTIVE &&
          latest_label[:user_id] == consignment_request[:consignment_package_sender_id]
          raise GoshPosh::Platform::Errors::InvalidConsignmentLabelGenerationRequestError
        end

        shipping_label_data = { reason: reason, shipping_label_type: shipping_label_type }
        consignment_request_machine.package_sender_shipment_label_generation_in_progress(access_context)
      else
        raise GoshPosh::Platform::Errors::InvalidConsignmentLabelGenerationRequestError
      end

      publish_shipping_messages(
        {
          type: 'new_consignment_shipping_label',
          user_id: user_id,
          shipping_label_data: shipping_label_data,
          consignment_request_id: consignment_request_id
        }
      )
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::CONSIGNMENT_PACKAGE_SENDER_LABEL_GENERATION_ERROR,
        __FILE__,
        __method__,
        { user_id: user_id, consignment_request_id: consignment_request_id } # attrs
      )
      raise error
    end

    def package_sender_assign_package_label_id(access_context, user_id, consignment_request_id, package_label_id)
      check_access(access_context[:access_token], user_id)
      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      if consignment_request.nil? || consignment_request[:consignment_package_sender_id] != user_id
        raise GoshPosh::Platform::Errors::ConsignmentRequestNotFoundError.new
      end
      package = validate_and_get_package_for_assignment(package_label_id)
      consignment_request_machine = GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      )
      consignment_request_machine.assign_consignment_package(access_context, package[:id], package[:package_label_id])

      consignment_request = consignment_request_machine.consignment_request
      services.consignment_service.update_consignment_package_request_id(
        consignment_request[:consignment_package_id],
        consignment_request[:id],
        [
          GoshPosh::Platform::Consignments::ConsignmentPackageState::NEW,
          GoshPosh::Platform::Consignments::ConsignmentPackageState::INACTIVE
        ]
      )
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::CONSIGNMENT_PACKAGE_SENDER_ASSIGN_PACKAGE_ERROR,
        __FILE__,
        __method__,
        { user_id: user_id, consignment_request_id: consignment_request_id, package_label_id: package_label_id } # attrs
      )
      raise error
    end

    def get_consignment_package_sender_requests(access_context, user_id, consignment_request_id)
      check_access(access_context[:access_token], user_id)
      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      if consignment_request.nil? || consignment_request[:consignment_package_sender_id] != user_id
        raise GoshPosh::Platform::Errors::ConsignmentRequestNotFoundError.new
      end
      supplier = services.user_service.by_id(consignment_request[:consignment_supplier_id])
      consignment_request_details = consignment_request.slice(*PACKAGE_SENDER_FIELDS)
      consignment_request_details[:consignment_supplier_user_info] = {
        id: consignment_request[:consignment_supplier_id],
        username: supplier[:username]
      }

      if consignment_request[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SENDER_SHIPMENT_LABEL_GENERATION_IN_PROGRESS
        consignment_request_details[:shipping_label_info] = {
          error_message: GoshPosh::Platform::Errors::ConsignmentErrorMessages::SHIPPING_LABEL_GENERATION_IN_PROGRESS
        }
      elsif [
        GoshPosh::Platform::Consignments::ConsignmentRequestState::AWAITING_PACKAGE_SENDER_SHIPMENT,
        GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SENDER_SHIPMENT_LABEL_GENERATION_FAILED,
        GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SENDER_SHIPMENT_LABEL_SERVICE_UNAVAILABLE
      ].include?(consignment_request[:state])
        label = services.shipping_service.get_latest_consignment_shipping_label(
          consignment_request[:id],
          nil,
          [],
          [ GoshPosh::Platform::Commerce::ShippingLabelReason::CONSIGNMENT_PACKAGE_SENDER_TO_PCS ],
          false,
          true
        )
        consignment_request_details[:shipping_label_info] = get_consignment_shipping_label_info(label, consignment_request[:id])
      end

      consignment_request_details.merge!(
        consignment_display_status(consignment_request, GoshPosh::Platform::ConsignmentActor::PACKAGE_SENDER)
      )
      package_sender_assigned_state_change = consignment_request[:state_history].find do |state_change|
        state_change[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_AWAITING_PACKAGE
      end
      consignment_request_details[:assigned_at] = package_sender_assigned_state_change[:created_at] if package_sender_assigned_state_change

      meta = {
        show_shipping_steps: false,
        show_download_shipping_label: false,
        show_generate_shipping_label: false,
        show_mark_as_shipped: false
      }
      setting = services.user_service.settings(user_id)
      meta[:shipping_label_type_preference] = setting ? setting[:shipping_label_type_preference] : GoshPosh::Platform::Commerce::ShippingLabelType::PRINTABLE

      if GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SENDER_GENERATE_LABEL_BUTTON_DISPLAY_STATES.include?(consignment_request[:state])
        meta[:show_shipping_steps] = true
        meta[:show_generate_shipping_label] = true
      elsif consignment_request[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::AWAITING_PACKAGE_SENDER_SHIPMENT
        meta[:show_shipping_steps] = true
        meta[:show_mark_as_shipped] = true
        meta[:show_download_shipping_label] = true
      end

      {
        data: consignment_request_details,
        meta: meta
      }
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::CONSIGNMENT_PACKAGE_SENDER_ASSIGN_PACKAGE_ERROR,
        __FILE__,
        __method__,
        { user_id: user_id, consignment_request_id: consignment_request_id } # attrs
      )
      raise error
    end

    def cancel_active_supplier_enrollment_request(user_id, actor_id)
      supplier_enrollment_request = services.consignment_service.get_consignment_supplier_enrollment_requests(
        user_id, GoshPosh::Platform::Consignments::ConsignmentSupplierEnrollmentState::ACTIVE_SUPPLIER_ENROLLMENT_STATES
      )

      if supplier_enrollment_request && !supplier_enrollment_request.empty?
        services.consignment_service.update_supplier_enrollment_request_state(
          user_id,
          GoshPosh::Platform::Consignments::ConsignmentSupplierEnrollmentState::CANCELLED,
          actor_id,
          GoshPosh::Platform::Consignments::ConsignmentSupplierEnrollmentState::ACTIVE_SUPPLIER_ENROLLMENT_STATES
        )
      end
    end

    def cancel_active_partner_enrollment_request(user_id, actor_id)
      partner_enrollment_request = services.consignment_service.get_consignment_partner_enrollment_requests(
        user_id, GoshPosh::Platform::Consignments::ConsignmentPartnerEnrollmentState::ACTIVE_PARTNER_ENROLLMENT_STATES
      )

      if partner_enrollment_request && !partner_enrollment_request.empty?
        services.consignment_service.update_partner_enrollment_request_state(
          user_id,
          GoshPosh::Platform::Consignments::ConsignmentPartnerEnrollmentState::CANCELLED,
          actor_id,
          GoshPosh::Platform::Consignments::ConsignmentPartnerEnrollmentState::ACTIVE_PARTNER_ENROLLMENT_STATES
        )
      end
    end

    def process_consignment_support_request_user_action(
      access_context, user_id, consignment_request_id, sr_id, consignment_actor_type, support_request_action
    )
      support_request_action = support_request_action.to_sym if support_request_action

      case support_request_action
      when GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::INVITE_AGENT
        user_invites_poshmark_agent_to_consignment_chat(
          access_context, user_id, consignment_request_id, sr_id, consignment_actor_type
        )
      when GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::RESOLVE_AGENT_INVITED_CHAT
        user_resolve_agent_invited_consignment_chat(
          access_context, user_id, consignment_request_id, sr_id, consignment_actor_type
        )
      else
        raise GoshPosh::Platform::Errors::ValidationError.new('Invalid Support Request Action')
      end
    end

    def create_support_case_for_invalid_consignment_package_id(access_context, user_id, consignment_request_id, package_id)
      check_access(access_context[:access_token], user_id)
      consignment_partner = services.user_service.by_id(user_id)
      return unless consignment_partner[:consignment_partner]

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      return unless consignment_request

      consignment_supplier = services.user_service.by_id(consignment_request[:consignment_supplier_id])
      case_labels = [GoshPosh::Platform::SupportCases::SupportCaseLabel::CONSIGNMENT_PACKAGE_SWAP]

      case_subject = "Regarding the Consignment package for the request \"#{consignment_request[:id]} mis-shipment for the partner @#{consignment_partner[:username]} and supplier @#{consignment_supplier[:username]}\""
      case_description = "The Consignment bag #{consignment_request[:id]} swapped with #{package_id} for \"@#{consignment_partner[:username]} and @#{consignment_supplier[:username]}\""
      case_info = services.support_case_service.case_with_subject_present?(case_subject)

      if case_info[:open_case_present] && case_info[:open_case_id]
        case_identifier = { service_cloud_id: case_info[:open_case_id] }
        update_attrs = {}
        update_attrs[GoshPosh::Platform::SupportCases::SupportCaseFields::PACKAGE_ID] = package_id
        update_attrs[GoshPosh::Platform::SupportCases::SupportCaseFields::CASE_NOTES] = "The Consignment bag #{consignment_request[:package_label_id]} swapped with #{package_id} for \"@#{consignment_partner[:username]} and @#{consignment_supplier[:username]}\" case updated at #{Time.now}"

        services.support_case_service.update_existing_case(
          case_identifier,
          update_attrs,
          GoshPosh::Platform::SupportCases::SupportCaseCategory::CONSIGNMENT_PACKAGE_SWAP,
          {}
        )
      else
        create_case_params = {
          support_case_category: GoshPosh::Platform::SupportCases::SupportCaseCategory::CONSIGNMENT_PACKAGE_SWAP,
          user_email: consignment_partner[:email],
          user_id: consignment_partner[:id],
          user_firstname: consignment_partner[:first_name],
          user_lastname: consignment_partner[:last_name],
          subject: case_subject,
          description: case_description,
          labels: case_labels,
          user_score: consignment_partner[:aggregates][:user_score],
          user_type: GoshPosh::Platform::Util.get_user_type(consignment_partner),
          custom_fields: {
            package_id: package_id,
            consignment_request_id: consignment_request_id,
            case_notes: "The Consignment bag #{consignment_request[:package_label_id]} swapped with #{package_id} for \"@#{consignment_partner[:username]} and @#{consignment_supplier[:username]}\" case created at #{Time.now}",
            system_case_type: GoshPosh::Platform::SupportCases::SystemCaseType::CONSIGNMENT_PACKAGE_SWAP
          }
        }
        services.support_case_service.create_new_case(create_case_params)
      end
    end

    def consignment_support_request_interactions_for_admin(access_context, consignment_request_id)
      admin_user = check_admin_access(access_context[:access_token])

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      raise GoshPosh::Platform::Errors::NotFoundError unless consignment_request

      support_requests = services.consignment_service.support_requests_by_consignment_request_id(
        consignment_request_id,
        sr_type: GoshPosh::Platform::Commerce::SupportRequestType::CONSIGNMENT_REQUEST_CHAT
      )
      interactions = []
      support_requests.each do |support_request|
        interactions.concat(
          consignment_support_chat_interactions(access_context, admin_user[:id], consignment_request, support_request)
        )
      end

      interactions = interactions.select { |message| message && message[:timestamp] }
      # Oldest first
      interactions = interactions.sort { |x, y| [y[:timestamp]] <=> [x[:timestamp]] }.reverse

      if interactions.any?
        active_agent_session = false
        interactions.each do |interaction|
          if [
            GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::INVITE_AGENT,
            GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::REPORT_CHAT
          ].include?(interaction[:action])
            interaction[:chat_section] = GoshPosh::Platform::Commerce::ConsignmentChatSection::SUPPLIER_PARTNER_AGENT
            active_agent_session = true
          elsif [
            GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::RESOLVE_AGENT_INVITED_CHAT,
            GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::RESOLVE_REPORT_CHAT
          ].include?(interaction[:action])
            interaction[:chat_section] = GoshPosh::Platform::Commerce::ConsignmentChatSection::SUPPLIER_PARTNER_AGENT
            active_agent_session = false
          elsif active_agent_session
            interaction[:chat_section] = GoshPosh::Platform::Commerce::ConsignmentChatSection::SUPPLIER_PARTNER_AGENT
          else
            interaction[:chat_section] = GoshPosh::Platform::Commerce::ConsignmentChatSection::SUPPLIER_PARTNER
          end
        end

        admin_user_ids = interactions.collect { |interaction| interaction[:actor_id] }.compact.uniq

        if admin_user_ids
          user_ref_cache = user_references(admin_user_ids)
          interactions.each do |interaction|
            if user_ref_cache[interaction[:actor_id]]
              interaction[:actor_name] = user_ref_cache[interaction[:actor_id]][:full_name]
            end
          end
        end
      end

      {
        support_requests: support_requests,
        support_request_interactions: interactions,
        consignment_supplier: get_user_as_admin(access_context[:access_token], consignment_request[:consignment_supplier_id]),
        consignment_partner: get_user_as_admin(access_context[:access_token], consignment_request[:consignment_partner_id]),
        consignment_request: consignment_request
      }
    end

    def process_admin_action_on_consignment_support_request(
      access_context, consignment_request_id, sr_id, action, request_params
    )
      case action.to_s
      when GoshPosh::Platform::Commerce::SupportRequestAdminActions::CONSIGNMENT_ADD_CHAT_MESSAGE
        admin_add_message_to_consignment_support_request(
          access_context,
          consignment_request_id,
          sr_id,
          { user_message: request_params[:consignment_chat_comment] }
        )
      when GoshPosh::Platform::Commerce::SupportRequestAdminActions::CONSIGNMENT_RESOLVE_REPORTED_CHAT
        admin_resolve_reported_consignment_chat(access_context, consignment_request_id, sr_id)
      when GoshPosh::Platform::Commerce::SupportRequestAdminActions::CONSIGNMENT_RESOLVE_INVITE_AGENT_CHAT
        admin_resolve_invite_agent_consignment_chat(access_context, consignment_request_id, sr_id)
      when GoshPosh::Platform::Commerce::SupportRequestAdminActions::CONSIGNMENT_TEMPORARILY_RESOLVE_CHAT
        admin_temporarily_resolve_consignment_chat(access_context, consignment_request_id, sr_id)
      else
        raise GoshPosh::Platform::Errors::ValidationError.new('Invalid Support Request Action')
      end
    end

    def update_consignment_chat_status_as_admin(access_context, consignment_request_id, chat_status)
      admin = services.user_service.by_id(access_context[:access_token].identity)
      check_user_permissions(
        access_context,
        [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_CHAT],
        admin
      )

      GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request_id
      ).update_chat_status(access_context, chat_status)
    end

    def mark_all_consignment_supplier_chat_messages_as_read(access_context, supplier_id, consignment_request_id)
      supplier_id = check_access(access_context[:access_token], supplier_id)

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      raise consignment_chat_unable_to_process_error unless consignment_request

      unless supplier_id == consignment_request[:consignment_supplier_id]
        raise consignment_chat_unable_to_process_error
      end

      services.consignment_service.reset_consignment_request_supplier_unread_chat_messages_count(consignment_request_id)
    end

    def mark_all_consignment_partner_chat_messages_as_read(access_context, partner_id, consignment_request_id)
      partner_id = check_access(access_context[:access_token], partner_id)

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      raise consignment_chat_unable_to_process_error unless consignment_request

      unless partner_id == consignment_request[:consignment_partner_id]
        raise consignment_chat_unable_to_process_error
      end

      services.consignment_service.reset_consignment_request_partner_unread_chat_messages_count(consignment_request_id)
    end

    def update_partner_discarded_items_count(access_context, partner_id, consignment_request_id, discarded_items_count)
      partner_id = check_access(access_context[:access_token], partner_id)
      discarded_items_count = discarded_items_count.to_i

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      unless consignment_request && partner_id == consignment_request[:consignment_partner_id] &&
             consignment_request[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::INVENTORY_IN_PROCESS &&
             !discarded_items_count.negative?
        raise consignment_invalid_input_try_again_error
      end

      services.consignment_service.update_consignment_request_discarded_items_count(
        consignment_request_id, discarded_items_count
      )
    end

    def admin_onboard_consignment_partner(access_context, user_id, consignment_partner_params)
      admin = services.user_service.by_id(access_context[:access_token].identity)
      check_user_permissions(
        access_context,
        [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_USERS],
        admin
      )

      user = services.user_service.by_id(user_id)
      unless user[:consignment_partner]
        raise GoshPosh::Platform::Errors::InvalidInputError.new('User is not a consignment partner')
      end

      consignment_schedule_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_SCHEDULE,
        user[:home_domain]
      )
      unless consignment_schedule_fs && consignment_schedule_fs[:enabled]
        raise GoshPosh::Platform::Errors::InvalidInputError.new('Consignment schedule is not enabled for this user')
      end

      if consignment_partner_params[:address_notes]
        validate_consignment_address_notes(
          user, consignment_partner_params[:address_notes], actor: GoshPosh::Platform::ConsignmentActor::PARTNER
        )
      end

      update_params = {
        capacity: consignment_schedule_fs[:default_weekly_partner_capacity],
        package_frequency: GoshPosh::Platform::Consignments::ConsignmentPartnerPackageFrequency::WEEKLY,
        address_notes: consignment_partner_params[:address_notes]
      }
      if consignment_partner_params.key?(:empty_package_sender_enabled)
        update_params[:empty_package_sender_enabled] = (
          consignment_partner_params[:empty_package_sender_enabled] == 'true' ||
            consignment_partner_params[:empty_package_sender_enabled] == true
        )
      end
      if consignment_partner_params.key?(:empty_package_available_count)
        consignment_partner_params[:empty_package_available_count].to_i.tap do |empty_package_count|
          if empty_package_count.negative?
            raise GoshPosh::Platform::Errors::InvalidInputError.new('Empty package count can\'t be negative')
          else
            update_params[:empty_package_available_count] = empty_package_count
          end
        end
      end
      unless services.consignment_service.upsert_consignment_partner_info(
        user[:id],
        update_params,
        actor_id: access_context[:access_token].identity,
        enable_partner_probation: consignment_schedule_fs[:enable_partner_probation]
      )
        raise GoshPosh::Platform::Errors::InvalidInputError.new('Failed to create consignment partner info')
      end

      update_consignment_partner_index(user[:id])
      reassign_scheduled_abandoned_consignment_requests_async

      true
    end

    def available_consignment_schedules(access_context, supplier_id, consignment_request_id)
      supplier_id = check_access(access_context[:access_token], supplier_id)

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      unless supplier_id == consignment_request[:consignment_supplier_id]
        raise GoshPosh::Platform::Errors::InvalidConsignmentRequestError.new(
          GoshPosh::Platform::Errors::ValidationErrorMessages::UNABLE_TO_PROCESS_REQUEST_CONTACT_SUPPORT
        )
      end
      schedules = available_consignment_pickup_schedules(access_context, consignment_request)

      {
        data: {
          consignment_supplier_address: consignment_request[:consignment_supplier_address],
          schedules: schedules
        },
        presentation: supplier_schedules_presentation(consignment_request, schedules)
      }
    end

    def consignment_user_info_for_admin(access_context, user_id)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_USERS])

      user = services.user_service.by_id(user_id)
      consignment_user_info =
        if user[:consignment_partner]
          consignment_partner_info_for_admin(user)
        elsif user[:consignment_supplier]
          consignment_supplier_info_for_admin(user)
        else
          {}
        end

      { data: consignment_user_info }
    end

    def update_consignment_partner_info(access_context, user_id, partner_params)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_USERS])

      admin_id = access_context[:access_token].identity

      user = services.user_service.by_id(user_id)
      unless user[:consignment_partner]
        raise GoshPosh::Platform::Errors::InvalidInputError.new('User is not a consignment partner')
      end

      consignment_schedule_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_SCHEDULE,
        user[:home_domain]
      )

      if partner_params[:address_notes]
        validate_consignment_address_notes(
          user, partner_params[:address_notes], actor: GoshPosh::Platform::ConsignmentActor::PARTNER
        )
      end

      if partner_params[:capacity]
        partner_params[:capacity] = partner_params[:capacity].to_i
        unless partner_params[:capacity] >= 0 && partner_params[:capacity] <= consignment_schedule_fs[:max_capacity_per_partner]
          raise GoshPosh::Platform::Errors::InvalidInputError.new("Capacity must be a number between 0 and #{consignment_schedule_fs[:max_capacity_per_partner]}")
        end
      end

      if partner_params[:empty_package_count]
        partner_params[:empty_package_count] = partner_params[:empty_package_count].to_i
        unless partner_params[:empty_package_count] >= 0 && partner_params[:empty_package_count] <= consignment_schedule_fs[:max_empty_package_count_per_package_sender]
          raise GoshPosh::Platform::Errors::InvalidInputError.new("Empty package can be updated between 0 and #{consignment_schedule_fs[:max_empty_package_count_per_package_sender]}")
        end
      end

      if partner_params[:empty_package_sender_status]
        partner_params[:empty_package_sender_status] = partner_params[:empty_package_sender_status].to_s == 'true'
      end

      if partner_params[:state]
        unless services.consignment_service.update_consignment_partner_state(
          user_id,
          { state: partner_params[:state].to_sym, reason: partner_params[:reason].to_s },
          admin_id
        )
          raise consignment_invalid_input_try_again_error
        end

        if partner_params[:state].to_sym == GoshPosh::Platform::Consignments::ConsignmentPartnerState::DISABLED
          abandon_consignment_requests_when_partner_is_disabled(user_id)
        end

        begin
          services.event_logger_v2.consignment_partner_update_settings(
            access_context,
            { consignment_partner_state: partner_params[:state] },
            { id: user_id }, # partner
            admin_id
          )
        rescue => log_error
          services.logger.error GoshPosh::Platform::Util.print_stack_trace(
            "Error in #{__method__} while logging event user_id: #{user_id}", log_error
          )
        end
      end

      if partner_params[:weekly_capacity].nil?
        unless services.consignment_service.upsert_consignment_partner_info(
          user_id,
          {
            capacity: partner_params[:capacity],
            address_notes: partner_params[:address_notes],
            empty_package_available_count: partner_params[:empty_package_count],
            empty_package_sender_enabled: partner_params[:empty_package_sender_status]
          },
          actor_id: access_context[:access_token].identity
        )
          raise GoshPosh::Platform::Errors::InvalidInputError.new('Failed to update consignment partner info')
        end
      end

      if partner_params[:capacity]
        updated_week_numbers = []
        current_time = Time.now
        10.times do |week_offset|
          itr_week_number = consignment_week_number(current_time + week_offset.weeks)
          capacity_info = services.consignment_service.consignment_partner_capacity(
            user[:id], itr_week_number
          )
          if capacity_info
            services.consignment_service.update_partner_capacity(
              user[:id],
              itr_week_number,
              from_capacities: {},
              to_capacities: {
                total_capacity: partner_params[:capacity]
              }
            )
            updated_week_numbers << itr_week_number
          end
        end

        reassign_scheduled_abandoned_consignment_requests_async(
          consignment_request_ids: abandon_consignment_requests_when_partner_capacity_changes(
            user_id, updated_week_numbers
          )
        )
      end

      if partner_params[:weekly_capacity]
        week_capacity = partner_params[:weekly_capacity][:week_capacity].to_i
        week_number = partner_params[:weekly_capacity][:week_number].to_i
        unless week_capacity >= 0 && week_capacity <= 10
          raise GoshPosh::Platform::Errors::InvalidInputError.new('Capacity must be a number between 0 and 10')
        end

        admin_update_consignment_partner_weekly_capacity(access_context, user[:id], week_number, week_capacity)
      end

      update_consignment_partner_index(user_id)

      abandoned_package_requests =
        if partner_params[:empty_package_sender_status] == false
          abandon_consignment_package_requests_when_package_sender_becomes_unavailable(user_id)
        else
          []
        end
      assign_package_senders_for_awaiting_consignment_requests_async(
        consignment_request_ids: abandoned_package_requests
      )
    end

    def admin_get_filtered_consignment_requests(access_context, filter_params, sort_params, page, count)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      consignment_requests = []
      if filter_params[:consignment_request_id]
        if BSON::ObjectId.legal?(filter_params[:consignment_request_id])
          services.consignment_service.get_consignment_request_details(filter_params[:consignment_request_id])&.tap do |consignment_request|
            consignment_requests << consignment_request
          end
        end
      else
        search_query = { filters: {}, sort: [], count: count, page: page }
        filter_params.each do |key, value|
          search_query[:filters][key.to_sym] = value if value
        end

        GoshPosh::Platform::Consignments::SearchHelper::USERNAME_DATA_FILTERS.each do |key|
          next unless search_query[:filters][key]

          begin
            user_id_from_username = user_id_from_user(search_query[:filters][key])
            if user_id_from_username
              search_query[:filters]["#{key}_id".to_sym] = user_id_from_username
              search_query[:filters].delete(key)
            else
              # input is neither a BSON::ObjectId nor a valid username
              # intentionally not clearing the filter, otherwise we may run empty OS query
            end
          rescue GoshPosh::Platform::Errors::NotFoundError
            # Ignore
          end
        end

        if sort_params&.any? && sort_params.is_a?(Hash)
          field, direction = sort_params.first
          search_query[:sort] << [field.to_sym, direction.to_sym]
        else
          search_query[:sort] << [:created_at, :desc]
        end
        consignment_requests.concat(
          services.consignment_service.consignment_requests_from_query(search_query, opts: {})[:data]
        )
      end

      consignment_requests = consignment_requests.collect do |consignment_request|
        GoshPosh::Platform::Consignments::ConsignmentRequest.admin_summary(consignment_request)
      end

      user_references_cache = consignment_user_references(
        consignment_requests, %i[consignment_supplier_id consignment_partner_id consignment_package_sender_id]
      )
      consignment_requests.each do |consignment_request|
        consignment_request[:consignment_supplier_username] =
          user_references_cache[consignment_request[:consignment_supplier_id]]&.dig(:username)
        consignment_request[:consignment_supplier_first_name] =
          user_references_cache[consignment_request[:consignment_supplier_id]]&.dig(:first_name)
        consignment_request[:consignment_supplier_last_name] =
          user_references_cache[consignment_request[:consignment_supplier_id]]&.dig(:last_name)
        consignment_request[:consignment_partner_username] =
          user_references_cache[consignment_request[:consignment_partner_id]]&.dig(:username)
        consignment_request[:consignment_package_sender_username] =
          user_references_cache[consignment_request[:consignment_package_sender_id]]&.dig(:username)
      end

      consignment_requests
    end

    def partner_package_unprocessed_consignment_requests(partner_id)
      services.consignment_service.consignment_requests_from_query(
        {
          filters: {
            consignment_partner_id: partner_id,
            states: GoshPosh::Platform::Consignments::ConsignmentRequestState::PARTNER_PACKAGE_UNPROCESSED_STATES
          },
          sort: { created_at: :desc }
        },
        opts: { id_only: true, limit: 1 }
      )[:data]
    rescue
      raise GoshPosh::Platform::Errors::ValidationError.new(
        GoshPosh::Platform::Errors::ValidationErrorMessages::UNABLE_TO_PROCESS_REQUEST
      )
    end

    def find_open_consignment_requests_by_address(consignment_address, search_limit)
      services.consignment_service.consignment_requests_from_query(
        {
          filters: {
            consignment_address: {
              street: consignment_address&.dig(:street),
              street2: consignment_address&.dig(:street2),
              city: consignment_address&.dig(:city),
              state: consignment_address&.dig(:state),
              country: consignment_address&.dig(:country),
              zip: consignment_address&.dig(:zip)
            },
            states: GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_OPEN_REQUESTS_STATES
          },
          sort: { created_at: :desc }
        },
        opts: { default_count: search_limit }
      )[:data]
    rescue => error
      services.logger.error GoshPosh::Platform::Util.print_stack_trace(__method__.to_s, error)
      []
    end

    def find_open_consignment_requests_by_partner_address(consignment_address, search_limit)
      services.consignment_service.consignment_requests_from_query(
        {
          filters: {
            consignment_partner_address: {
              street: consignment_address&.dig(:street),
              street2: consignment_address&.dig(:street2),
              city: consignment_address&.dig(:city),
              state: consignment_address&.dig(:state),
              country: consignment_address&.dig(:country),
              zip: consignment_address&.dig(:zip)
            },
            states: GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_OPEN_REQUESTS_STATES
          },
          sort: { created_at: :desc }
        },
        opts: { default_count: search_limit }
      )[:data]
    rescue => error
      services.logger.error GoshPosh::Platform::Util.print_stack_trace(__method__.to_s, error)
      []
    end

    def consignment_action_with_retry(action:, max_tries:, critical_action: true)
      current_try = 1
      begin
        action_result = yield

        raise GoshPosh::Platform::Errors::ConsignmentActionFailedError unless action_result && action_result[:success]
      rescue GoshPosh::Platform::Errors::ConsignmentActionFailedError
        services.logger.warn("#{__method__}: #{action}, try: #{current_try}/#{max_tries}")
        if current_try < max_tries
          current_try += 1
          retry
        end
        services.error("#{__method__}: all (#{max_tries}) tries failed for #{action}")
        GoshPosh::Runtime::PmLogger.instance.pm_error(
          action,
          __FILE__,
          __method__,
          [GoshPosh::Runtime::PmLog::Tags::CONSIGNMENTS] # tags
        )

        raise if critical_action
      end

      action_result && action_result[:result]
    end

    def admin_assign_package_senders_for_awaiting_consignment_requests_async(access_context)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      assign_package_senders_for_awaiting_consignment_requests_async
    end

    def admin_reassign_scheduled_abandoned_consignment_requests_async(access_context)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      reassign_scheduled_abandoned_consignment_requests_async
    end

    def admin_assign_tpl_package_sender_async(access_context, cr_ids)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      assign_tpl_package_sender_async(consignment_request_ids: cr_ids)
    end

    def admin_trigger_partner_matching_async(access_context)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      trigger_partner_matching_async
    end

    def get_consignment_qr_code_image_url(access_context, consignment_qr_code_request_id)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])
      consignment_qr_code_request = services.consignment_service.get_consignment_qr_code_request(consignment_qr_code_request_id)

      case consignment_qr_code_request[:qr_code_type]
      when GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_GENERAL_QR_CODE, GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_PACKAGE_GENERIC_QR_CODE
        consignment_qr_code_request[:qr_code_image][:url]
      when GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_PACKAGE_SPECIFIC_QR_CODE
        GoshPosh::Platform::Util.get_media_url(consignment_qr_code_request[:qr_code_zip_file][:path], consignment_qr_code_request[:qr_code_zip_file][:storage_location])
      end
    end

    def get_consignment_qr_code_url_csv_url(access_context, consignment_qr_code_request_id)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])
      consignment_qr_code_request = services.consignment_service.get_consignment_qr_code_request(consignment_qr_code_request_id)
      GoshPosh::Platform::Util.get_media_url(consignment_qr_code_request[:qr_code_url_csv][:path], consignment_qr_code_request[:qr_code_url_csv][:storage_location])
    end

    def admin_requests_consignment_qr_code(access_context, actor_id, qr_code_type, comment, utm_source, ad_partner, qr_code_count)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])
      consignment_qr_code_request = services.consignment_service.create_consignment_qr_code_request(actor_id, qr_code_type, comment, utm_source, ad_partner, qr_code_count)

      case consignment_qr_code_request[:qr_code_type]
      when GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_GENERAL_QR_CODE, GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_PACKAGE_GENERIC_QR_CODE
        payload = {
          type: :generate_consignment_qr_code,
          consignment_qr_code_request_id: consignment_qr_code_request[:id],
          qr_code_type: qr_code_type,
          utm_source: utm_source,
          ad_partner: ad_partner
        }
        GoshPosh::Platform::QueueHelper.publish_message(GoshPosh::Settings.queue_settings.queues.admin, payload)
      when GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_PACKAGE_SPECIFIC_QR_CODE
        GoshPosh::Workers::ConsignmentQRCodeWorker.schedule(
          nil,
          { consignment_qr_code_request_id: consignment_qr_code_request[:id] },
          services.logger,
          Time.now + GoshPosh::Settings.consignment_qr_code_sidekiq_delay_seconds.to_i
        )

        qr_code_count.times do
          payload = {
            type: :generate_consignment_qr_code,
            consignment_qr_code_request_id: consignment_qr_code_request[:id],
            qr_code_type: qr_code_type
          }
          GoshPosh::Platform::QueueHelper.publish_message(GoshPosh::Settings.queue_settings.queues.admin, payload)
        end
      end
    end

    def get_consignment_qr_code_requests(access_context, page, count)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])
      consignment_qr_code_requests = services.consignment_service.get_consignment_qr_code_requests({}, page, count)
      consignment_qr_code_requests_count = services.consignment_service.get_consignment_qr_code_requests_count({})
      {
        consignment_qr_code_requests: consignment_qr_code_requests,
        consignment_qr_code_requests_count: consignment_qr_code_requests_count
      }
    end

    def generate_consignment_general_qr_code(consignment_qr_code_request_id, utm_source, ad_partner)
      url = get_url_for_consignment_qr_code(GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_GENERAL_QR_CODE, utm_source, ad_partner, nil)
      qr_code_image = create_consignment_qr_code_image(url, GoshPosh::Platform::Users::QR_TYPE_CONSIGNMENT_GENERAL, nil)
      services.consignment_service.update_qr_code_image_to_request(consignment_qr_code_request_id, qr_code_image)
    end

    def generate_consignment_package_generic_qr_code(consignment_qr_code_request_id, utm_source, ad_partner)
      url = get_url_for_consignment_qr_code(GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_PACKAGE_GENERIC_QR_CODE, utm_source, ad_partner, nil)
      qr_code_image = create_consignment_qr_code_image(url, GoshPosh::Platform::Users::QR_TYPE_CONSIGNMENT_PACKAGE_GENERIC, nil)
      services.consignment_service.update_qr_code_image_to_request(consignment_qr_code_request_id, qr_code_image)
    end

    def generate_consignment_package_specific_qr_code(consignment_qr_code_request_id)
      package_label_id = nil
      loop do
        package_label_id = services.consignment_service.new_package_label_id
        break unless services.consignment_service.get_package_by_package_label_id(package_label_id)
      end

      url = get_url_for_consignment_qr_code(GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_PACKAGE_SPECIFIC_QR_CODE, nil, nil, package_label_id)
      qr_code_image = create_consignment_qr_code_image(url, GoshPosh::Platform::Users::QR_TYPE_CONSIGNMENT_PACKAGE_SPECIFIC, package_label_id)
      consignment_package = services.consignment_service.create_consignment_package(package_label_id, qr_code_image)
      services.consignment_service.create_consignment_qr_code_request_to_package(consignment_qr_code_request_id, consignment_package[:id])
      services.consignment_service.inc_completed_qr_code_count(consignment_qr_code_request_id)
    end

    def get_url_for_consignment_qr_code(qr_code_type, utm_source, ad_partner, package_label_id)
      home_domain = GoshPosh::Platform::Metadata::Domain::UNITED_STATES
      url = nil
      case qr_code_type
      when GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_GENERAL_QR_CODE
        url = "#{GoshPosh::Platform::Util.get_domain_based_poshmark_url(home_domain)}/consignment/?utm_source=#{utm_source}"
        url += "&ad_partner=#{ad_partner}" if ad_partner
      when GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_PACKAGE_GENERIC_QR_CODE
        url = "#{GoshPosh::Platform::Util.get_domain_based_poshmark_url(home_domain)}/consignment/?package_label_id=generic&utm_source=#{utm_source}"
        url += "&ad_partner=#{ad_partner}" if ad_partner
      when GoshPosh::Platform::Consignments::ConsignmentQRCodeType::CONSIGNMENT_PACKAGE_SPECIFIC_QR_CODE
        url = "#{GoshPosh::Platform::Util.get_domain_based_poshmark_url(home_domain)}/consignment/?package_label_id=#{package_label_id}"
      end
      url
    end

    def create_consignment_qr_code_image(url, qr_type, package_label_id)
      id = BSON::ObjectId.new
      qr = RQRCode::QRCode.new(url)
      tmp_file_path = "/goshposh/server/uploads/tmp/consignment_qr_code_image_#{id}.png"
      qr_size = 400
      qr.as_png(size: qr_size)
        .save(tmp_file_path)

      body = Tilt.new('app/views/apps_common/qr_code_templates/consignment_qr_code_image.slim')
      width = qr_size
      case qr_type
      when GoshPosh::Platform::Users::QR_TYPE_CONSIGNMENT_GENERAL, GoshPosh::Platform::Users::QR_TYPE_CONSIGNMENT_PACKAGE_GENERIC
        height = width
      when GoshPosh::Platform::Users::QR_TYPE_CONSIGNMENT_PACKAGE_SPECIFIC
        height = width * 1.2
      end
      logo_height = 100
      font_size = 75
      html_str = body.render(
        Object.new,
        {
          width: width,
          height: height,
          package_label_id: package_label_id,
          qr_img_path: tmp_file_path,
          logo_height: logo_height,
          logo_left: qr_size / 2 - logo_height / 2,
          logo_top: qr_size / 2 - logo_height / 2,
          package_label_id_top: qr_size - 25,
          font_size: font_size
        }
      )
      kit = GoshPosh::Platform::ImageConverter::HtmlToImage.new(html_str, width, height)
      kit_tmp_file_path = "/goshposh/server/uploads/tmp/consignment_qr_code_image_#{id}_with_additions.png"
      file = kit.save_to_file(kit_tmp_file_path)
      file.close unless file.closed?
      File.delete(tmp_file_path)
      qr_code_image = GoshPosh::Platform::Users::QRCodeImage.create(id, kit_tmp_file_path, qr_type)
      File.delete(kit_tmp_file_path)
      qr_code_image
    end

    def calculate_address_coordinates(user_id, address, update_address_book: false)
      return unless address&.any?

      validated_address = services.shipping_service.validate_shipping_address(
        address,
        nil, # address_type
        consignment_address_validator(address) # validation_provider_override
      )

      coordinates = validated_address[:coordinates]

      if coordinates&.any?
        address[:coordinates] = coordinates

        if update_address_book
          begin
            services.order_service.update_address_to_user_address_book(user_id, address, address[:id], {})
          rescue => error
            services.logger.warn(
              GoshPosh::Platform::Util.print_stack_trace(
                "#{__method__} Error while updating address coordinates for user_id: #{user_id}, address_id: #{address[:id]}", error
              )
            )
          end
        end
      else
        services.logger.warn(
          "#{__method__} Could not calculate geo coordinates for user_id: #{user_id}, address_id: #{address[:id]}"
        )
      end

      address
    end

    def initialise_consignment_partner_info(access_context, user_id, consignment_partner_params)
      if access_context[:user_id] == GoshPosh::Platform::POSHMARK_ID
        actor_id = GoshPosh::Platform::POSHMARK_ID
      else
        admin = services.user_service.by_id(access_context[:access_token].identity)
        check_user_permissions(
          access_context,
          [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_USERS],
          admin
        )
        actor_id = access_context[:access_token].identity
      end

      user = services.user_service.by_id(user_id)
      if user[:consignment_supplier]
        raise GoshPosh::Platform::Errors::NotAllowedError.new(
          GoshPosh::Platform::Errors::ValidationErrorMessages::CONSIGNMENT_PARTNER_SUPPLIER_MISMATCH
        )
      end
      if user[:consignment_partner]
        raise GoshPosh::Platform::Errors::NotAllowedError.new(
          GoshPosh::Platform::Errors::ValidationErrorMessages::USER_ALREADY_CONSIGNMENT_PARTNER
        )
      end

      consignment_schedule_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_SCHEDULE,
        user[:home_domain]
      )
      unless consignment_schedule_fs && consignment_schedule_fs[:enabled]
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::CONSIGNMENT_SCHEDULE_NOT_ENABLED
        )
      end

      address_notes = consignment_partner_params[:address_notes] ||= ''
      if address_notes
        validate_consignment_address_notes(user, address_notes, actor: GoshPosh::Platform::ConsignmentActor::PARTNER)
      end

      partner_capacity = (consignment_partner_params[:capacity] || consignment_schedule_fs[:default_weekly_partner_capacity]).to_i
      if partner_capacity.negative?
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::INVALID_CAPACITY
        )
      end

      unless services.user_service.get_unmasked_phone_number_for_user(user_id)
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::INVALID_USER_PHONE_NUMBER
        )
      end

      consignment_address = services.order_service.default_consignment_address_in_user_address_list(user_id)
      unless consignment_address
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::INVALID_USER_ADDRESS
        )
      end
      unless consignment_address[:coordinates]&.any?
        calculate_address_coordinates(user_id, consignment_address, update_address_book: true)
      end

      update_params = {
        capacity: partner_capacity,
        package_frequency: GoshPosh::Platform::Consignments::ConsignmentPartnerPackageFrequency::WEEKLY,
        address_notes: address_notes
      }
      if consignment_partner_params.key?(:empty_package_sender_enabled)
        update_params[:empty_package_sender_enabled] = (
          consignment_partner_params[:empty_package_sender_enabled] == 'true' ||
            consignment_partner_params[:empty_package_sender_enabled] == true
        )
      else
        update_params[:empty_package_sender_enabled] = consignment_schedule_fs[:default_empty_package_sender_enabled_value]
      end
      if consignment_partner_params.key?(:empty_package_available_count)
        consignment_partner_params[:empty_package_available_count].to_i.tap do |empty_package_count|
          if empty_package_count.negative?
            raise GoshPosh::Platform::Errors::InvalidInputError.new(
              GoshPosh::Platform::Errors::ConsignmentErrorMessages::EMPTY_PACKAGE
            )
          else
            update_params[:empty_package_available_count] = empty_package_count
          end
        end
      end
      unless services.consignment_service.upsert_consignment_partner_info(
        user[:id],
        update_params,
        actor_id: actor_id,
        enable_partner_probation: consignment_schedule_fs[:enable_partner_probation]
      )
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::CONSIGNMENT_PARTNER_CREATION_FAILED
        )
      end

      if consignment_schedule_fs[:enable_partner_capacities_pre_population]
        consignment_schedule_fs[:partner_capacities_pre_population_weeks].times do |week_offset|
          current_week_number = consignment_week_number(Time.now + week_offset.weeks)
          capacity_info = services.consignment_service.consignment_partner_capacity(
            user_id, current_week_number
          )

          if capacity_info
            services.consignment_service.update_partner_capacity(
              user_id,
              current_week_number,
              from_capacities: {
                total_capacity: capacity_info[:total_capacity]
              },
              to_capacities: {
                total_capacity: partner_capacity
              }
            )
          else
            services.consignment_service.create_consignment_partner_capacity(
              user_id,
              current_week_number,
              week_start_at: week_number_to_consignment_weekly_time_window(current_week_number).first,
              total_capacity: partner_capacity
            )
          end
        end
      end

      update_consignment_partner_index(user[:id])
      reassign_scheduled_abandoned_consignment_requests_async
    end

    def active_consignment_package_data_by_package_id(access_context, user_id, package_label_id)
      user_id = check_access(access_context[:access_token], user_id)

      package = services.consignment_service.get_package_by_package_label_id(
        package_label_id, states: [GoshPosh::Platform::Consignments::ConsignmentPackageState::ACTIVE]
      )
      return { data: [] } unless package && package[:consignment_request_id]

      services.consignment_service.get_consignment_request_details(package[:consignment_request_id])&.tap do |request|
        if [:consignment_supplier_id, :consignment_partner_id, :consignment_package_sender_id].any? { |key| request[key] == user_id }
          return { data: [package.except(:state_history)] }
        end
      end

      { data: [] }
    rescue => error
      log_consignment_api_error(
        error, GoshPosh::Runtime::PmLog::ErrorName::CONSIGNMENT_PACKAGE_DETAILS_LOOKUP_ERROR, __FILE__, __method__
      )

      { data: [] }
    end

    def post_tpl_package_sender_assigned(successful_request_ids_count, failed_request_ids_count, total_process_request_ids_count, start_time, method_name, reason: nil)
      link = GoshPosh::Platform::Util.construct_method_name_and_trace_id_kibana_url(method_name)
      time_diff = (Time.now - start_time).to_i
      summary_message_components = []

      summary_message_components << "#{successful_request_ids_count} CRs were assigned with 3PL package sender out of #{total_process_request_ids_count} awaiting CRs"
      summary_message_components << "Total Failure Count: #{failed_request_ids_count}"
      summary_message_components << "Kibana Logs: <#{link}|Logs>"
      summary_message_components << "Total Time Taken: #{Time.at(time_diff).utc.strftime('%M:%S').to_s}"
      post_consignment_ops_update_to_slack(summary_message_components)
    end

    def schedule_bulk_add_consignment_tracking_info(access_context, tracking_infos)
      check_user_permissions(access_context, [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_REQUESTS])

      tracking_infos.each do |tracking_info|
        GoshPosh::Platform::QueueHelper.publish_message(
          GoshPosh::Settings.queue_settings.queues.admin,
          {
            type: :add_consignment_tracking_info,
            tracking_info: tracking_info,
            admin_id: access_context[:access_token].identity
          }
        )
      end
    end

    def update_consignment_partner_weekly_capacity_state(access_context, user_id, week_number, state)
      user_id = check_access(access_context[:access_token], user_id)
      user = services.user_service.by_id(user_id)

      raise consignment_invalid_input_try_again_error unless user[:consignment_partner]

      week_number = week_number.to_i # TODO: validate range, cant update past week
      state = state.to_sym

      unless GoshPosh::Platform::Consignments::ConsignmentPartnerCapacityState.valid?(state)
        raise consignment_invalid_input_try_again_error
      end

      if state == GoshPosh::Platform::Consignments::ConsignmentPartnerCapacityState::ENABLED &&
         consignment_wind_down_enabled?(access_context, user_id)
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::PARTNER_CAPACITY_OPT_IN_DISABLED
        )
      end

      capacity_info = services.consignment_service.consignment_partner_capacity(user_id, week_number)
      if capacity_info.nil?
        partner_info = services.consignment_service.consignment_partner_info(user_id)
        services.consignment_service.create_consignment_partner_capacity(
          user_id,
          week_number,
          week_start_at: week_number_to_consignment_weekly_time_window(week_number).first,
          total_capacity: partner_info[:capacity],
          scheduled_capacity: 0,
          matched_capacity: 0,
          state: state,
          actor_id: user_id
        )
      else
        if capacity_info[:scheduled_capacity].positive? || capacity_info[:matched_capacity].positive?
          raise consignment_invalid_input_try_again_error
        end

        if capacity_info[:state] != state
          services.consignment_service.update_partner_capacity_state_existing_doc(
            user_id, week_number, state: state, actor_id: user_id
          )
        end
      end

      begin
        settings = {
          week_start_date: week_number_to_consignment_weekly_time_window(week_number).first
        }
        if state == GoshPosh::Platform::Consignments::ConsignmentPartnerCapacityState::ENABLED
          settings[:capacity_opt_in] = true
        else
          settings[:capacity_opt_out] = true
        end
        services.event_logger_v2.consignment_partner_update_settings(access_context, settings, user)
      rescue => log_error
        services.logger.error GoshPosh::Platform::Util.print_stack_trace(
          "Error in #{__method__} while logging event user_id: #{user_id}", log_error
        )
      end
    end

    def find_nearby_notifiable_consignment_requests(
      coordinates:, cr_search_radius_miles:,
      id_only: false, page: 1
    )
      result = services.consignment_service.consignment_requests_from_query(
        {
          filters: {
            notify_capacity_availability: true,
            within_radius_miles: {
              distance: cr_search_radius_miles,
              lat: coordinates[:latitude],
              lon: coordinates[:longitude]
            }
          },
          sort: {
            notify_capacity_availability_updated_at: 'asc'
          },
          page: page
        },
        opts: { id_only: id_only }
      )
      result[:data] || []
    rescue => error
      services.logger.error GoshPosh::Platform::Util.print_stack_trace(
        "#{__method__} Notifiable Consignment Search Error", error
      )
      []
    end

    def add_consignment_request_inventory_image(access_context, user_id, consignment_request_id, image_file)
      user_id = check_access(access_context[:access_token], user_id)

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)

      unless consignment_request &&
             user_id == consignment_request[:consignment_supplier_id] &&
             GoshPosh::Platform::Consignments::ConsignmentRequestState::CONSIGNMENT_INVENTORY_IMAGES_ALLOWED_STATES.include?(consignment_request[:state])
        raise consignment_invalid_input_try_again_error
      end

      images_count = services.consignment_service.get_consignment_request_inventory_images_count(consignment_request_id)
      #TODO: Handle concurrent requests for future
      unless images_count < GoshPosh::Platform::Consignments::ConsignmentRequestInventoryImage::MAX_IMAGES_PER_CONSIGNMENT_REQUEST
        raise consignment_invalid_input_try_again_error
      end

      image_data = services.consignment_service.add_consignment_request_inventory_image(
        user_id, consignment_request_id, image_file
      )
      unless image_data && image_data[:image] && image_data[:image][:url]
        raise GoshPosh::Platform::Errors::PersistenceError.new('Failed to save image')
      end

      image_data
    end

    def get_consignment_request_inventory_images(access_context, user_id, consignment_request_id)
      user_id = check_access(access_context[:access_token], user_id)

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      unless consignment_request &&
             user_id == consignment_request[:consignment_supplier_id] &&
             GoshPosh::Platform::Consignments::ConsignmentRequestState::CONSIGNMENT_INVENTORY_IMAGES_ALLOWED_STATES.include?(consignment_request[:state])
        raise consignment_invalid_input_try_again_error
      end

      inventory_images = services.consignment_service.get_consignment_request_inventory_images(consignment_request_id)
      {
        data: inventory_images
      }
    end

    def delete_consignment_request_inventory_image(access_context, user_id, consignment_request_id, image_id)
      return unless image_id

      user_id = check_access(access_context[:access_token], user_id)

      consignment_request = services.consignment_service.get_consignment_request_details(consignment_request_id)
      unless consignment_request &&
             user_id == consignment_request[:consignment_supplier_id] &&
             GoshPosh::Platform::Consignments::ConsignmentRequestState::CONSIGNMENT_INVENTORY_IMAGES_ALLOWED_STATES.include?(consignment_request[:state])
        raise consignment_invalid_input_try_again_error
      end

      services.consignment_service.remove_consignment_request_inventory_image(image_id)
    end

    private

    def consignment_wind_down_enabled?(access_context, user_id)
      return false unless user_id

      app_consignments_fs = GoshPosh::Platform::Util.get_v3_feature_setting(
        :app_consignments,
        GoshPosh::Platform::Util.ensure_object_id(user_id),
        GoshPosh::Platform::FeatureSettings::FeatureActorType::USER
      )
      return false unless app_consignments_fs

      app_type = access_context[:app_type]&.to_s
      return false unless GoshPosh::Platform::AppTypes::MOBILE_TYPES.include?(app_type) ||
                          GoshPosh::Platform::AppTypes::WEB_TYPES.include?(app_type)

      app_platform = GoshPosh::Platform::APP_TYPE_TO_PLATFORM[app_type]&.to_sym
      app_consignments_fs.dig(app_platform, :is_consignment_wind_down_enabled) == true
    end

    def closed_supplier_enrollment_response(home_domain)
      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        home_domain
      )

      {
        data: [],
        meta: nil,
        presentation: enrollment_closed_presentation(consignment_fs)
      }
    end

    def validate_program_accepting_new_enrollment!(access_context, user)
      return unless consignment_wind_down_enabled?(access_context, user[:id])

      raise GoshPosh::Platform::Errors::InvalidInputError.new(
        GoshPosh::Platform::Errors::ConsignmentErrorMessages::ENROLLMENT_CLOSED
      )
    end

    def consignment_partner_info_for_admin(user)
      consignment_user_info = { user_type: 'Consignment Partner' }

      partner_info = services.consignment_service.consignment_partner_info(user[:id])
      consignment_user_info.merge!(partner_info) if partner_info&.any?

      consignment_schedule_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_SCHEDULE,
        user[:home_domain]
      )

      services.order_service.default_consignment_address_in_user_address_list(user[:id])&.tap do |consignment_address|
        consignment_user_info[:consignment_address] = consignment_address

        consignment_user_info[:nearby_partners] = []
        nearby_partner_infos = find_nearby_partners(
          address: consignment_address,
          search_radius: consignment_schedule_fs[:partner_search_radius_miles],
          user_id: user[:id]
        )
        partner_user_references = user_references(nearby_partner_infos.collect { |pi| pi[:id] })
        nearby_partner_infos.each do |partner_info|
          consignment_user_info[:nearby_partners] << {
            id: partner_info[:id],
            username: partner_user_references[partner_info[:id].to_s]&.dig(:username),
            capacity: partner_info[:capacity],
            last_bag_assignment: partner_info[:last_consignment_request_assigned_at]
          }
        end
      end

      current_time = Time.now
      consignment_user_info[:capacity_consumption] = []
      (-2..12).each do |week_offset|
        itr_week_number = consignment_week_number(current_time + week_offset.weeks)
        begin
          itr_weekly_time_window_array =
            GoshPosh::Platform::Consignments::ConsignmentScheduleHelper.week_number_to_consignment_weekly_time_window(
              itr_week_number, week_already_passed: week_offset.negative?
            )
          itr_begin_week_day = GoshPosh::Platform::Util.get_formatted_time(itr_weekly_time_window_array.first, :month_date_compact)
        rescue => error
          services.logger.warn GoshPosh::Platform::Util.print_stack_trace("#{__method__} #{user[:id]}", error)
          itr_begin_week_day = ''
        end
        weekly_capacity_info = effective_partner_weekly_capacity_info(
          user[:id], partner_info, itr_week_number, itr_weekly_time_window_array.first
        )
        consignment_user_info[:capacity_consumption] << {
          week: weekly_capacity_info[:id][:week_number],
          total: weekly_capacity_info[:total_capacity],
          state: weekly_capacity_info[:state],
          day: itr_begin_week_day,
          scheduled: weekly_capacity_info[:scheduled_capacity],
          matched: weekly_capacity_info[:matched_capacity]
        }
      end

      consignment_user_info
    end

    def admin_consignment_requests_summary(consignment_requests, consignment_references)
      consignment_requests.map do |request|
        request.slice(:id, :state, :created_at, :updated_at).merge(
          partner: consignment_references.dig(request[:consignment_partner_id], :username),
          package_sender: consignment_references.dig(request[:consignment_package_sender_id], :username),
          supplier: consignment_references.dig(request[:consignment_supplier_id], :username)
        ).compact
      end
    end

    def consignment_supplier_info_for_admin(user)
      consignment_user_info = { user_type: 'Consignment Supplier' }

      consignment_schedule_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_SCHEDULE,
        user[:home_domain]
      )

      services.order_service.default_consignment_address_in_user_address_list(user[:id])&.tap do |consignment_address|
        consignment_user_info[:consignment_address] = consignment_address

        consignment_user_info[:nearby_partners] = []
        nearby_partner_infos = find_nearby_partners(
          address: consignment_address,
          search_radius: consignment_schedule_fs[:partner_search_radius_miles],
          user_id: user[:id]
        )
        partner_user_references = user_references(nearby_partner_infos.collect { |pi| pi[:id] })
        nearby_partner_infos.each do |partner_info|
          consignment_user_info[:nearby_partners] << {
            id: partner_info[:id],
            username: partner_user_references[partner_info[:id].to_s]&.dig(:username),
            capacity: partner_info[:capacity],
            last_bag_assignment: partner_info[:last_consignment_request_assigned_at]
          }
        end
      end

      consignment_user_info
    end

    def get_consignment_shipping_label_info(label, cr_id)
      return unless label

      error_message = nil
      shipping_label_info = {}
      shipping_label_info[:shipping_label_type] = label[:shipping_label_type]

      if label[:state] == GoshPosh::Platform::Commerce::ShippingLabelState::ACTIVE
        case label[:shipping_label_type]
        when GoshPosh::Platform::Commerce::ShippingLabelType::QR_CODE
          shipping_label_info[:qr_code_label] = label[:qr_code_label]
        when GoshPosh::Platform::Commerce::ShippingLabelType::PRINTABLE
          shipping_label_info[:download_url] = construct_download_consignment_shipping_label_url(label, cr_id)
        end
      elsif label[:state] == GoshPosh::Platform::Commerce::ShippingLabelState::LABEL_GENERATE_FAILED
        error_message = GoshPosh::Platform::Errors::ConsignmentErrorMessages::SHIPPING_LABEL_GENERATION_FAILED
      elsif GoshPosh::Platform::Commerce::ShippingLabelState::DEACTIVATED_STATES.include?(label[:state])
        error_message = GoshPosh::Platform::Errors::ConsignmentErrorMessages::SHIPPING_LABEL_DEACTIVATED
      elsif label[:state] == GoshPosh::Platform::Commerce::ShippingLabelState::LABEL_SERVICE_UNAVAILABLE
        error_message = GoshPosh::Platform::Errors::ConsignmentErrorMessages::SHIPPING_LABEL_SERVICE_UNAVAILABLE
      else
        error_message = GoshPosh::Platform::Errors::ConsignmentErrorMessages::SHIPPING_LABEL_GENERATION_IN_PROGRESS
      end

      shipping_label_info[:error_message] = error_message if error_message
      shipping_label_info
    end

    def construct_download_consignment_shipping_label_url(label, cr_id)
      storage = GoshPosh::Platform::Storage::S3StorageV2.get_storage(GoshPosh::Platform::StorageContentV2::LABELS)
      pdf_path =
        if label[:pdf_label_packing_slip] && label[:pdf_label_packing_slip][:pdf_path]
          label[:pdf_label_packing_slip][:pdf_path]
        else
          label[:pdf_path]
        end

      return unless pdf_path

      storage.presigned_download_url(pdf_path, cr_id)
    end

    def validate_consignment_address_notes(user, notes, actor: GoshPosh::Platform::ConsignmentActor::SUPPLIER)
      return unless notes

      if notes.length > GoshPosh::Settings.consignment_address_notes_max_length
        case actor
        when GoshPosh::Platform::ConsignmentActor::SUPPLIER
          raise GoshPosh::Platform::Errors::InvalidInputError.new(
            'Pickup instructions cannot be longer than ' \
              "#{GoshPosh::Settings.consignment_address_notes_max_length} characters."
          )
        when GoshPosh::Platform::ConsignmentActor::PARTNER
          raise GoshPosh::Platform::Errors::InvalidInputError.new(
            'Drop-off instructions cannot be longer than ' \
              "#{GoshPosh::Settings.consignment_address_notes_max_length} characters."
          )
        else
          raise GoshPosh::Platform::Errors::InvalidInputError.new(
            'Instructions cannot be longer than ' \
              "#{GoshPosh::Settings.consignment_address_notes_max_length} characters."
          )
        end
      end

      check_comment_content(
        notes,
        user,
        user[:id],
        nil, # app_type
        nil, # app_version
        GoshPosh::Platform::CommentObjectTypes::CONSIGNMENT_ADDRESS_NOTES,
        [:offline] # filter_skip_list
      )
    end

    def admin_add_message_to_consignment_support_request(access_context, consignment_request_id, sr_id, message_params)
      admin = services.user_service.by_id(access_context[:access_token].identity)
      check_user_permissions(
        access_context,
        [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_CHAT],
        admin
      )

      consignment_request, support_request = validate_consignment_request_and_latest_support_request_link(
        consignment_request_id, sr_id
      )

      validate_consignment_chat_message_params(
        access_context, admin, message_params, consignment_request, is_admin: true
      )

      sr_actor_type = GoshPosh::Platform::Commerce::SupportRequestMessageActor::ADMIN
      updated_support_request = services.order_service.add_user_message_to_consignment_support_request(
        sr_id: support_request[:id],
        consignment_request_id: consignment_request[:id],
        actor_id: admin[:id],
        actor: sr_actor_type,
        message: message_params[:user_message]
      )
      raise consignment_chat_unable_to_process_error unless updated_support_request
      services.consignment_service.consignment_request_chat_message_added(consignment_request[:id], sr_actor_type)

      message_params_event_logger = {
        message: message_params[:user_message],
        actor: sr_actor_type
      }
      services.event_logger_v2.send_user_message_to_ind_or_consignment_chat(
        access_context,
        admin,
        consignment_request,
        message_params_event_logger,
        false,
        support_request[:id],
        GoshPosh::Platform::Commerce::SupportRequestType::CONSIGNMENT_REQUEST_CHAT,
        admin[:id]
      )

      agent_commented_on_consignment_chat_comms(consignment_request, admin)
    end

    def admin_resolve_reported_consignment_chat(access_context, consignment_request_id, sr_id)
      admin = services.user_service.by_id(access_context[:access_token].identity)
      check_user_permissions(
        access_context,
        [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_CHAT],
        admin
      )

      consignment_request, support_request = validate_consignment_request_and_latest_support_request_link(
        consignment_request_id, sr_id
      )

      return unless support_request[:data] && support_request[:data][:reported_by]

      message_actor = GoshPosh::Platform::Commerce::SupportRequestMessageActor::ADMIN
      action = GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::RESOLVE_REPORT_CHAT
      resolved_at = Time.now
      user_message = services.order_service.create_sr_user_message(
        message_actor, nil, nil, nil, action, resolved_at
      )
      update_params = {
        user_message: user_message,
        report_resolved_by: admin[:id],
        report_resolved_at: resolved_at,
        reported_reason: nil,
        reported_by: nil
      }
      updated_support_request = services.order_service.update_support_request_by_id(support_request[:id], update_params)

      GoshPosh::Platform::QueueHelper.publish_message(
        GoshPosh::Settings.queue_settings.queues.commerce,
        {
          type: :create_or_update_support_case_for_consignment_chat_event,
          consignment_request_id: consignment_request[:id],
          user_id: admin[:id],
          actor: GoshPosh::Runtime::EventActor::ADMIN,
          is_resolve_reported_chat: true,
          sr_id: sr_id,
          user_message: updated_support_request[:user_messages].last
        },
        false,
        access_context
      )
    end

    def admin_resolve_invite_agent_consignment_chat(access_context, consignment_request_id, sr_id)
      admin = services.user_service.by_id(access_context[:access_token].identity)
      check_user_permissions(
        access_context,
        [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_CHAT],
        admin
      )

      consignment_request, support_request = validate_consignment_request_and_latest_support_request_link(
        consignment_request_id, sr_id
      )

      return unless support_request[:data] && support_request[:data][:agent_invited_at] &&
                    support_request[:data][:agent_invited_by]

      message_actor = GoshPosh::Platform::Commerce::SupportRequestMessageActor::ADMIN
      action = GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::RESOLVE_AGENT_INVITED_CHAT
      resolved_at = Time.now
      user_message = services.order_service.create_sr_user_message(
        message_actor, nil, nil, nil, action, resolved_at
      )
      update_params = {
        user_message: user_message,
        agent_invited_resolved_by: admin[:id],
        agent_invited_resolved_at: resolved_at,
        agent_invited_by: nil,
        agent_invited_at: nil
      }
      updated_support_request = services.order_service.update_support_request_by_id(support_request[:id], update_params)

      GoshPosh::Platform::QueueHelper.publish_message(
        GoshPosh::Settings.queue_settings.queues.commerce,
        {
          type: :create_or_update_support_case_for_consignment_chat_event,
          consignment_request_id: consignment_request[:id],
          user_id: admin[:id],
          actor: GoshPosh::Runtime::EventActor::ADMIN,
          is_resolve_agent_invited_chat: true,
          sr_id: sr_id,
          user_message: updated_support_request[:user_messages].last
        },
        false,
        access_context
      )
    end

    def admin_temporarily_resolve_consignment_chat(access_context, consignment_request_id, sr_id)
      admin = services.user_service.by_id(access_context[:access_token].identity)
      check_user_permissions(
        access_context,
        [GoshPosh::Platform::UserPermission::MANAGE_CONSIGNMENT_CHAT],
        admin
      )

      consignment_request, support_request = validate_consignment_request_and_latest_support_request_link(
        consignment_request_id, sr_id
      )

      unless [
        GoshPosh::Platform::Commerce::SupportRequestStatus::NEW,
        GoshPosh::Platform::Commerce::SupportRequestStatus::OPEN
      ].include?(support_request[:status].to_sym)
        raise GoshPosh::Platform::Errors::SupportRequestValidationError.new(
          'Support request is in invalid status.' # admin facing copy
        )
      end

      updated_support_request = services.order_service.update_support_request_by_id(
        support_request[:id],
        { status: GoshPosh::Platform::Commerce::SupportRequestStatus::PENDING }
      )

      return unless updated_support_request && updated_support_request[:service_cloud_id]

      GoshPosh::Platform::QueueHelper.publish_message(
        GoshPosh::Settings.queue_settings.queues.commerce,
        {
          type: :update_support_case_for_consignment_chat,
          support_case_id: updated_support_request[:service_cloud_id],
          consignment_request_id: consignment_request[:id],
          status: GoshPosh::Platform::Commerce::DeskCaseStatus::RESOLVED
        },
        false,
        access_context
      )
    end

    def validate_consignment_chat_message_params(
      access_context, actor, chat_params, consignment_request, is_admin: false
    )
      if (!chat_params[:pictures] || chat_params[:pictures].empty?) &&
        (!chat_params[:user_message] || chat_params[:user_message].empty?)
        raise GoshPosh::Platform::Errors::SupportRequestValidationError.new(
          GoshPosh::Platform::Errors::OrderErrorMessages::ORDER_CHAT_REQUEST_MISSING
        )
      end

      # TODO: validate chat_params[:pictures].size

      support_request_message_validation(
        access_context,
        consignment_request[:id],
        actor,
        chat_params[:user_message],
        GoshPosh::Platform::CommentObjectTypes::CONSIGNMENT_REQUEST_CHAT,
        is_admin
      )
    end

    def user_invites_poshmark_agent_to_consignment_chat(
      access_context, user_id, consignment_request_id, sr_id, consignment_actor_type
    )
      user_id = check_access(access_context[:access_token], user_id)

      inviting_user, consignment_request, support_request = validate_user_performing_consignment_chat_action(
        user_id,
        consignment_request_id,
        sr_id,
        chat_action: GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::INVITE_AGENT,
        consignment_actor_type: consignment_actor_type
      )

      if support_request[:data] && support_request[:data][:agent_invited_by]
        raise GoshPosh::Platform::Errors::SupportRequestValidationError.new(
          GoshPosh::Platform::Errors::SupportRequestErrorMessages::AGENT_ALREADY_INVITED
        )
      end

      case consignment_actor_type
      when GoshPosh::Platform::ConsignmentActor::SUPPLIER
        inviting_sr_actor_type = GoshPosh::Platform::Commerce::SupportRequestMessageActor::CONSIGNMENT_SUPPLIER
        counter_party_sr_actor_type = GoshPosh::Platform::Commerce::SupportRequestMessageActor::CONSIGNMENT_PARTNER
        inviting_user_id = consignment_request[:consignment_supplier_id]
        counter_party_user_id = consignment_request[:consignment_partner_id]
      when GoshPosh::Platform::ConsignmentActor::PARTNER
        inviting_sr_actor_type = GoshPosh::Platform::Commerce::SupportRequestMessageActor::CONSIGNMENT_PARTNER
        counter_party_sr_actor_type = GoshPosh::Platform::Commerce::SupportRequestMessageActor::CONSIGNMENT_SUPPLIER
        inviting_user_id = consignment_request[:consignment_partner_id]
        counter_party_user_id = consignment_request[:consignment_supplier_id]
      else
        raise consignment_chat_unable_to_process_error
      end

      deflection = consignment_chat_user_invite_agent_deflection(user_id, support_request, counter_party_sr_actor_type)
      if deflection[:state] == GoshPosh::Platform::Commerce::OrderChatInviteAgentState::DISABLED
        raise GoshPosh::Platform::Errors::SupportRequestError.new(deflection[:message])
      end

      invited_at = Time.now
      user_message = services.order_service.create_sr_user_message(
        inviting_sr_actor_type,
        nil, # message
        nil, # reason
        nil, # pictures
        GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::INVITE_AGENT,
        invited_at
      )
      updated_support_request = services.order_service.update_support_request_by_id(
        sr_id,
        {
          user_message: user_message,
          agent_invited_by: inviting_user_id,
          agent_invited_at: invited_at,
          agent_invited_resolved_by: nil,
          agent_invited_resolved_at: nil
        }
      )

      user_invited_agent_on_consignment_chat_comms(
        consignment_request,
        actor_id: inviting_user_id,
        recipient_id: counter_party_user_id,
        recipient_sr_actor_type: counter_party_sr_actor_type
      )

      GoshPosh::Platform::QueueHelper.publish_message(
        GoshPosh::Settings.queue_settings.queues.commerce,
        {
          type: :create_or_update_support_case_for_consignment_chat_event,
          consignment_request_id: consignment_request_id,
          user_id: user_id,
          is_invite_agent: true,
          sr_id: sr_id,
          user_message: updated_support_request[:user_messages].last
        },
        false,
        access_context
      )

      updated_support_request
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::USER_PERFORM_CONSIGNMENT_CHAT_ACTION_ERROR,
        __FILE__,
        __method__,
        {
          user_id: user_id, sr_id: sr_id, consignment_request_id: consignment_request_id,
          action: GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::INVITE_AGENT
        }
      )

      raise
    end

    def user_resolve_agent_invited_consignment_chat(
      access_context, user_id, consignment_request_id, sr_id, consignment_actor_type
    )
      user_id = check_access(access_context[:access_token], user_id)

      user, consignment_request, support_request = validate_user_performing_consignment_chat_action(
        user_id,
        consignment_request_id,
        sr_id,
        chat_action: GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::RESOLVE_AGENT_INVITED_CHAT,
        consignment_actor_type: consignment_actor_type
      )

      unless support_request[:data] && support_request[:data][:agent_invited_by] == user_id &&
             support_request[:data][:agent_invited_resolved_by].nil?
        raise consignment_chat_unable_to_process_error
      end

      inviting_sr_actor_type = consignment_actor_type_to_sr_actor_type(consignment_actor_type)
      agent_invited_resolved_at = Time.now
      user_message = services.order_service.create_sr_user_message(
        inviting_sr_actor_type,
        nil, # message
        nil, # reason
        nil, # pictures
        GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::RESOLVE_AGENT_INVITED_CHAT,
        agent_invited_resolved_at
      )
      updated_support_request = services.order_service.update_support_request_by_id(
        sr_id,
        {
          user_message: user_message,
          agent_invited_resolved_by: user_id,
          agent_invited_resolved_at: agent_invited_resolved_at,
          agent_invited_by: nil,
          agent_invited_at: nil
        }
      )
      unless updated_support_request
        raise consignment_chat_unable_to_process_error
      end

      GoshPosh::Platform::QueueHelper.publish_message(
        GoshPosh::Settings.queue_settings.queues.commerce,
        {
          type: :create_or_update_support_case_for_consignment_chat_event,
          consignment_request_id: consignment_request_id,
          user_id: user_id,
          is_resolve_agent_invited_chat: true,
          sr_id: sr_id,
          user_message: updated_support_request[:user_messages].last
        },
        false,
        access_context
      )

      updated_support_request
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::USER_PERFORM_CONSIGNMENT_CHAT_ACTION_ERROR,
        __FILE__,
        __method__,
        {
          user_id: user_id, sr_id: sr_id, consignment_request_id: consignment_request_id,
          action: GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::RESOLVE_AGENT_INVITED_CHAT
        }
      )

      raise
    end

    def add_reported_consignment_chat_support_request(access_context, reported_by, support_request, reported_reason)
      check_access(access_context[:access_token], reported_by)

      if reported_reason.nil? || !GoshPosh::Platform::Commerce::OrderChatReportReason::ALL_ORDER_CHAT_REPORT_REASONS.include?(reported_reason.to_sym)
        raise GoshPosh::Platform::Errors::SupportRequestValidationError.new(
          GoshPosh::Platform::Errors::OrderErrorMessages::REPORT_CHAT_REASON_INVALID
        )
      end

      user, consignment_request, support_request = validate_user_performing_consignment_chat_action(
        reported_by,
        support_request[:consignment_request_id],
        support_request[:id],
        chat_action: GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::REPORT_CHAT,
        consignment_actor_type: nil
      )

      sr_actor_type = consignment_sr_actor_type_by_user_id(access_context, consignment_request, reported_by)
      reported_at = Time.now
      user_message = services.order_service.create_sr_user_message(
        sr_actor_type,
        nil, # message
        nil, # reason
        nil, # pictures
        GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::REPORT_CHAT,
        reported_at,
        GoshPosh::Platform::Commerce::SupportRequestMessageVisibility::HIDDEN
      )

      updated_support_request = services.order_service.update_support_request_by_id(
        support_request[:id],
        {
          user_message: user_message,
          reported_by: reported_by,
          reported_reason: reported_reason,
          report_resolved_by: nil,
          report_resolved_at: nil
        }
      )

      GoshPosh::Platform::Consignments::ConsignmentRequestMachine.new(
        services, services.logger, consignment_request[:id]
      ).update_chat_status(access_context, GoshPosh::Platform::Consignments::ChatStatus::DISABLED)

      GoshPosh::Platform::QueueHelper.publish_message(
        GoshPosh::Settings.queue_settings.queues.commerce,
        {
          type: :create_or_update_support_case_for_consignment_chat_event,
          consignment_request_id: consignment_request[:id],
          user_id: reported_by,
          is_report_chat: true,
          sr_id: support_request[:id],
          user_message: updated_support_request[:user_messages].last
        },
        false,
        access_context
      )
    rescue => error
      log_consignment_api_error(
        error,
        GoshPosh::Runtime::PmLog::ErrorName::USER_PERFORM_CONSIGNMENT_CHAT_ACTION_ERROR,
        __FILE__,
        __method__,
        {
          user_id: reported_by, sr_id: support_request&.dig(:di),
          action: GoshPosh::Platform::Commerce::SupportRequestOrderChatActions::REPORT_CHAT
        }
      )

      raise
    end

    def reopen_temporarily_resolved_support_case(access_context, consignment_request, support_request_pre_update)
      return unless support_request_pre_update && support_request_pre_update[:status] &&
        support_request_pre_update[:status].to_sym == GoshPosh::Platform::Commerce::SupportRequestStatus::PENDING

      services.order_service.update_support_request_by_id(
        support_request_pre_update[:id],
        { status: GoshPosh::Platform::Commerce::SupportRequestStatus::OPEN }
      )

      GoshPosh::Platform::QueueHelper.publish_message(
        GoshPosh::Settings.queue_settings.queues.commerce,
        {
          type: :update_support_case_for_consignment_chat,
          support_case_id: support_request_pre_update[:service_cloud_id],
          consignment_request_id: consignment_request[:id],
          status: GoshPosh::Platform::Commerce::DeskCaseStatus::OPEN
        },
        false,
        access_context
      )
    end

    NON_CRITICAL_CONSIGNMENT_ERRORS = [
      GoshPosh::Platform::Errors::AuthError,
      GoshPosh::Platform::Errors::NotFoundError,
      GoshPosh::Platform::Errors::ServiceUnavailable,
      Pm::Rbac::Errors::InsufficientPrivilegesError,
      GoshPosh::Platform::Errors::SupportRequestError,
      GoshPosh::Platform::Errors::SupportRequestValidationError
    ].freeze

    def log_consignment_api_error(error, logging_error_type, file, method, attrs = {})
      return if error.respond_to?(:user_message) && error.user_message
      return if NON_CRITICAL_CONSIGNMENT_ERRORS.any? { |non_critical_error| error.is_a?(non_critical_error) }

      services.logger.error GoshPosh::Platform::Util.print_stack_trace(
        "#{method}: #{attrs}", error
      )

      GoshPosh::Runtime::PmLogger.instance.pm_error(
        logging_error_type,
        file,
        method,
        [GoshPosh::Runtime::PmLog::Tags::CONSIGNMENTS],
        attrs
      )
    end

    def consignment_posts(access_context, post_ids, statuses = GoshPosh::Platform::Posts::PostStatus::CONSIGNMENT_POST_STATUSES)
      posts = services.post_service.posts_by_ids_v2(
        post_ids, statuses
      )

      populate_posts_for_client(posts.values, access_context, false, true)

      # using "post_ids" to build the array to preserve the order
      hidden_posts = []
      visible_posts = []
      post_ids.reverse.each do |post_id|
        post = posts[post_id]
        next unless post # Skip if no post is found
        if post[:status] == GoshPosh::Platform::Posts::PostStatus::HIDDEN
          hidden_posts << post
        else
          visible_posts << post
        end
      end
      { visible_posts: visible_posts, hidden_posts: hidden_posts }
    end

    def consignment_cover_shots(consignment_requests)
      cover_post_ids = consignment_requests.map { |consignment_request| consignment_request[:post_ids][0] }
      posts = services.post_service.posts_by_ids_v2(
        cover_post_ids, GoshPosh::Platform::Posts::PostStatus::CONSIGNMENT_POST_STATUSES
      )

      cover_shots = {}
      consignment_requests.each do |consignment_request|
        first_post_id = consignment_request[:post_ids][0]
        next unless first_post_id

        cover_shots[consignment_request[:id]] = posts[first_post_id]&.dig(:cover_shot)
      end

      cover_shots
    end

    def consignment_user_references(consignment_requests, user_id_keys)
      user_ids = Set.new
      user_id_keys.each do |user_id_key|
        consignment_requests.each do |consignment_request|
          user_ids << consignment_request[user_id_key]
        end
      end

      user_references(user_ids.to_a, { su_level: true, cover_shot: true })
    end

    def consignment_package_details(consignment_request)
      package_details = {}

      if consignment_request[:consignment_package_id]
        consignment_package = services.consignment_service.get_consignment_package_details(
          consignment_request[:consignment_package_id]
        )

        package_details.merge!(consignment_package.slice(*CONSIGNMENT_PACKAGE_FIELDS))
      end

      package_details
    end

    def consignment_display_status(consignment_request, consignment_actor)
      if GoshPosh::Platform::Consignments::ConsignmentRequest.items_sold_out?(consignment_request)
        display_state = GoshPosh::Platform::Consignments::ConsignmentRequestState::INVENTORY_SOLD_OUT
      else
        display_state = consignment_request[:state]
      end

      display_status = case consignment_actor
                       when GoshPosh::Platform::ConsignmentActor::PARTNER
                         GoshPosh::Platform::Consignments::ConsignmentRequestState::PARTNER_DISPLAY_STATUS[display_state]
                       when GoshPosh::Platform::ConsignmentActor::SUPPLIER
                         supplier_display_status = GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_DISPLAY_STATUS[display_state]
                         if supplier_display_status == GoshPosh::Platform::Consignments::ConsignmentRequestSupplierDisplayState::AWAITING_PICKUP_SCHEDULE &&
                            consignment_request.dig(:supplier_shipment_collection_info, :pickup_start_at)
                           supplier_display_status = GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_DISPLAY_STATUS[
                             GoshPosh::Platform::Consignments::ConsignmentRequestState::PICKUP_SCHEDULED
                           ]
                         end
                         supplier_display_status
                       when GoshPosh::Platform::ConsignmentActor::PACKAGE_SENDER
                         if package_sender_shipped_empty_package_to_supplier?(consignment_request)
                           GoshPosh::Platform::Consignments::PackageSenderDisplayStatus::SHIPPED
                         elsif consignment_request[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::CANCELLED
                            GoshPosh::Platform::Consignments::PackageSenderDisplayStatus::CANCELED
                         else
                           GoshPosh::Platform::Consignments::PackageSenderDisplayStatus::TO_SHIP
                         end
                       end

      if consignment_actor == GoshPosh::Platform::ConsignmentActor::PACKAGE_SENDER
        status_icon = GoshPosh::Platform::Consignments::PackageSenderDisplayStatus::STATUS_ICON[display_status]
      else
        status_icon = GoshPosh::Platform::Consignments::ConsignmentRequestState::STATUS_ICON[consignment_request[:state]]
      end

      {
        display_status: display_status,
        status_icon: GoshPosh::Platform::AssetHelpers.get_digest_image_url(status_icon)
      }
    end

    def check_consignment_request_access(viewer_home_domain)
      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        viewer_home_domain
      )
      return if consignment_fs&.dig(:show_consignment_requests)

      raise GoshPosh::Platform::Errors::ServiceUnavailable.new(
        'Whoops! Something went wrong. Please try again later.'
      )
    end

    def check_consignment_request_creation(supplier_home_domain)
      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        supplier_home_domain
      )
      return if consignment_fs&.dig(:create_request)

      raise GoshPosh::Platform::Errors::ServiceUnavailable.new(
        'Consignment Requests Service not available.' # Admin facing error
      )
    end

    def show_bags_to_ship_module_for_consignment_partner?(home_domain, partner_user_id)
      fs = GoshPosh::FeatureSettings.get_domain_based_v3(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_BAGS_TO_SHIP,
        home_domain,
        partner_user_id,
        GoshPosh::Platform::FeatureSettings::FeatureActorType::USER
      )
      fs&.dig(:enabled) == true
    end

    def validate_and_update_consignment_request_addresses!(supplier_home_domain, consignment_request_data)
      address_id = consignment_request_data.dig(:consignment_supplier_address, :id)
      if address_id
        address = services.order_service.get_address_from_user_address_list(
          consignment_request_data[:consignment_supplier_id],
          address_id
        )
        consignment_request_data.delete(:consignment_supplier_address) unless address&.fetch(:status, nil) == GoshPosh::Platform::Commerce::UserAddressStatus::VISIBLE
      end

      if consignment_request_data[:consignment_supplier_address]
        supplier_country_code = GoshPosh::Platform::Util.get_country_from_domain(supplier_home_domain)
        consignment_supplier_address = GoshPosh::Platform::Util.normalize_address(
          consignment_request_data[:consignment_supplier_address]
        )

        unless supplier_country_code == consignment_supplier_address[:country].to_sym
          raise GoshPosh::Platform::Errors::InvalidInputError
        end

        validate_address(consignment_supplier_address, GoshPosh::Platform::Commerce::UserAddressType::SHIPPING)
        consignment_request_data[:consignment_supplier_address] = consignment_supplier_address

        unless consignment_request_data[:consignment_supplier_address][:coordinates]&.any?
          address_with_coordinates = calculate_address_coordinates(
            consignment_request_data[:consignment_supplier_id],
            consignment_request_data[:consignment_supplier_address],
            update_address_book: false
          )
          consignment_request_data[:consignment_supplier_address] = address_with_coordinates
        end

        supplier_address = services.order_service.add_address_to_user_address_list(
          consignment_request_data[:consignment_supplier_id],
          consignment_request_data[:consignment_supplier_address],
          [GoshPosh::Platform::Commerce::DefaultAddress::CONSIGNMENT], :address_obj
        )
        consignment_request_data[:consignment_supplier_address] = supplier_address[:address]
                                                                  .merge!({ id: supplier_address[:id] })
      end

      if consignment_request_data[:consignment_partner_address]
        consignment_partner_address = GoshPosh::Platform::Util.normalize_address(
          consignment_request_data[:consignment_partner_address]
        )

        unless supplier_country_code == consignment_partner_address[:country].to_sym
          raise GoshPosh::Platform::Errors::InvalidInputError
        end

        validate_address(consignment_partner_address, GoshPosh::Platform::Commerce::UserAddressType::SHIPPING)
        consignment_request_data[:consignment_partner_address] = consignment_partner_address
      end
    end

    def consignment_state_transition_details(consignment_request)
      submitted_state_change = consignment_request[:state_history].find do |state_change|
        state_change[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::SUBMITTED
      end

      awaiting_package_state_change = consignment_request[:state_history].find do |state_change|
        state_change[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_AWAITING_PACKAGE
      end

      shipped_to_supplier_state_change = consignment_request[:state_history].find do |state_change|
        state_change[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::PACKAGE_SHIPPED_TO_SUPPLIER
      end

      delivered_state_change = consignment_request[:state_history].find do |state_change|
        state_change[:state] == GoshPosh::Platform::Consignments::ConsignmentRequestState::PENDING_INVENTORY_PROCESSING
      end

      {
        submitted_at: submitted_state_change&.dig(:created_at),
        awaiting_package_at: awaiting_package_state_change&.dig(:created_at),
        shipped_to_supplier_at: shipped_to_supplier_state_change&.dig(:created_at),
        delivered_at: delivered_state_change&.dig(:created_at)
      }
    end

    def supplier_consignment_requests_partner_info(consignment_request, consignment_partner_references)
      # Adding this check to not show the partner info in the UI for the consignment requests before in transit to partner states.
      state_number = GoshPosh::Platform::Consignments::ConsignmentRequestState::STATE_NUMBERS[consignment_request[:state]]
      if state_number&.>= 150
        { consignment_partner_user_info: consignment_partner_references[consignment_request[:consignment_partner_id]] }
      else
        { consignment_partner_user_info: nil }
      end
    end

    def validate_and_get_consignment_package(package_label_id)
      if package_label_id.strip.empty? ||
        !GoshPosh::Platform::Consignments::ConsignmentPackageDisplayId::CHARSET_REGEX.match?(package_label_id)
        raise GoshPosh::Platform::Errors::InvalidConsignmentLabelIDError
      end

      package = services.consignment_service.get_or_create_consignment_package_by_label_id(package_label_id)

      if package[:consignment_request_id]
        raise GoshPosh::Platform::Errors::InvalidConsignmentLabelIDError.new(
          "Consignment label ID already in use for request #{package[:consignment_request_id]}"
        )
      end

      package
    end

    def validate_and_get_package_for_assignment(package_label_id)
      if package_label_id && !package_label_id.strip.empty?
        package_label_id.upcase!
      else
        raise GoshPosh::Platform::Errors::InvalidConsignmentLabelIDError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::INVALID_BAG_ID
        )
      end

      unless GoshPosh::Platform::Consignments::ConsignmentPackageDisplayId::CHARSET_REGEX.match?(package_label_id)
        raise GoshPosh::Platform::Errors::InvalidConsignmentLabelIDError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::INVALID_BAG_ID
        )
      end

      package = services.consignment_service.get_package_by_package_label_id(package_label_id)
      valid_states = [GoshPosh::Platform::Consignments::ConsignmentPackageState::NEW,
                     GoshPosh::Platform::Consignments::ConsignmentPackageState::INACTIVE]
      if package.nil? || package[:consignment_request_id] || !valid_states.include?(package[:state])
        raise GoshPosh::Platform::Errors::InvalidConsignmentLabelIDError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::INVALID_BAG_ID
        )
      end

      package
    end

    def validate_consignment_partner(partner, consignment_request)
      if partner.nil?
        raise GoshPosh::Platform::Errors::InvalidConsignmentRequestError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::PARTNER_NOT_FOUND
        )
      end

      if partner[:consignment_partner] == false
        raise GoshPosh::Platform::Errors::InvalidConsignmentRequestError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::USER_IS_NOT_A_PARTNER
        )
      end

      if consignment_request[:consignment_supplier_id] == partner[:id]
        raise GoshPosh::Platform::Errors::InvalidConsignmentRequestError.new(
          GoshPosh::Platform::Errors::ConsignmentErrorMessages::SAME_SUPPLIER_AND_PARTNER
        )
      end

      partner
    end

    def validate_consignment_partner_address(partner_address, consignment_request)
      validate_address(partner_address, GoshPosh::Platform::Commerce::UserAddressType::SHIPPING)

      supplier = services.user_service.by_id(consignment_request[:consignment_supplier_id])
      supplier_country = GoshPosh::Platform::Util.get_country_from_domain(supplier[:home_domain])

      if supplier_country != partner_address[:country].to_sym
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          "Partner address country #{partner_address[:country]} does not match supplier country #{supplier_country}"
        )
      end
      validate_address_po_box(partner_address, is_consignment: true)
    end

    def get_latest_request_for_supplier(supplier_id)
      previous_request = nil
      previous_request_id = services.consignment_service.get_supplier_request_ids(supplier_id, nil, 1).first

      if previous_request_id
        previous_request = services.consignment_service.get_consignment_request_details(previous_request_id)
      end

      previous_request
    end

    def get_consignment_request_address(supplier_id, user_address_id = nil, auth_session_id = nil)
      address_list = services.order_service.get_user_address_list(supplier_id)
      address = nil

      if user_address_id
        address = services.order_service.get_address_from_user_address_list(supplier_id, user_address_id)
      end

      if address.nil?
        user_address_list = services.order_service.get_user_address_list(supplier_id)
        address = user_address_list&.dig(:consignment_address) ||
          user_address_list&.dig(:shipping_address) ||
          user_address_list&.dig(:return_address)
      end

      if address.nil?
        user_latest_commerce_info = services.order_service.get_user_latest_commerce_info(supplier_id)
        if user_latest_commerce_info && user_latest_commerce_info[:shipping_address_id]
          user_address_id = user_latest_commerce_info[:shipping_address_id]
          address = services.order_service.get_address_from_user_address_list(supplier_id, user_address_id)
        end
      end

      begin
        validate_address_po_box(address, is_consignment: true) if address&.any?
      rescue => e
        address = nil
      end

      if auth_session_id
        addr_id = address[:id]
        matching_addr = address_list[:addresses].find { |address| address[:id] == addr_id }
        if matching_addr && matching_addr[:auth_session_id] != auth_session_id
          address = nil
        end
      end

      if address.nil?
        raise GoshPosh::Platform::Errors::InvalidInputError.new(
          "Something went wrong, Please try again later"
        )
      end

      calculate_address_coordinates(supplier_id, address, update_address_book: false)
      address
    end

    def get_open_requests_count(supplier_id)
      max_pagination_count = GoshPosh::Settings.max_consignment_requests_count

      # We currently cannot filter requests by status from mongo directly as it will do a collection scan.
      # So taking a certain number of requests and filtering them.
      request_ids = services.consignment_service.get_supplier_request_ids(supplier_id, nil, max_pagination_count)

      requests = services.consignment_service.get_consignment_requests_details(request_ids, [])

      requests.count do |request|
        GoshPosh::Platform::Consignments::ConsignmentRequestState::STATE_NUMBERS[request[:state]] <
          GoshPosh::Platform::Consignments::ConsignmentRequestState::STATE_NUMBERS[
            GoshPosh::Platform::Consignments::ConsignmentRequestState::PENDING_INVENTORY_PROCESSING
          ]
      end
    end

    def admin_update_consignment_partner_weekly_capacity(access_context, user_id, week_number, week_capacity)
      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        access_context[:home_domain]
      )

      capacity_info = services.consignment_service.consignment_partner_capacity(
        user_id, week_number
      )
      old_matched_capacity = capacity_info&.dig(:matched_capacity)
      if capacity_info
        services.consignment_service.update_partner_capacity(
          user_id,
          week_number,
          from_capacities: {},
          to_capacities: {
            total_capacity: week_capacity
          }
        )
      else
        services.consignment_service.create_consignment_partner_capacity(
          user_id,
          week_number,
          week_start_at: week_number_to_consignment_weekly_time_window(week_number).first,
          total_capacity: week_capacity,
          scheduled_capacity: 0,
          matched_capacity: 0
        )
      end

      if old_matched_capacity > week_capacity && consignment_fs[:rematch_consignment_requests_on_capacity_reduction]
        partner = services.user_service.by_id(user_id)
        access_context[:home_domain] ||= partner[:home_domain]
        handle_hard_matched_crs_when_capacity_reduced(
          access_context,
          user_id,
          week_number,
          week_capacity
        )
      end

      reassign_scheduled_abandoned_consignment_requests_async(
        consignment_request_ids: abandon_consignment_requests_when_partner_capacity_changes(
          user_id, [week_number]
        )
      )

      begin
        settings = {
          week_start_date: week_number_to_consignment_weekly_time_window(week_number).first,
          capacity: week_capacity
        }
        services.event_logger_v2.consignment_partner_update_settings(
          access_context,
          settings,
          { id: user_id }, # partner
          access_context[:access_token].identity # admin_id
        )
      rescue => log_error
        services.logger.error GoshPosh::Platform::Util.print_stack_trace(
          "Error in #{__method__} while logging event user_id: #{user_id}", log_error
        )
      end
    end

    def validate_consignment_request_creation(access_context, consignment_supplier_id, consignment_fs)
      enforce_rate_limits_v2(
        :rate_limits,
        :create_consignment_request,
        "th|u:{#{consignment_supplier_id}}|c:cr",
        GoshPosh::Settings.throttles[:create_consignment_request][:rate_limits],
        true
      ) do
        consignment_request_address = get_consignment_request_address(consignment_supplier_id)

        if consignment_fs[:limit_open_consignments_with_same_address]
          max_open_consignments_with_same_address = consignment_fs[:max_open_consignments_with_same_address]
          open_consignment_request_by_address = find_open_consignment_requests_by_address(
            consignment_request_address, max_open_consignments_with_same_address
          )

          open_consignment_request_by_address = open_consignment_request_by_address.select do |request|
            GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_OPEN_REQUESTS_STATES.include?(request[:state])
          end

          if open_consignment_request_by_address.length >= max_open_consignments_with_same_address
            GoshPosh::Runtime::PmLogger.instance.pm_error(
              GoshPosh::Runtime::PmLog::ErrorName::OPEN_CONSIGNMENT_REQUESTS_WITH_SAME_ADDRESS_LIMIT_REACHED_ERROR,
              __FILE__,
              __method__,
              [GoshPosh::Runtime::PmLog::Tags::CONSIGNMENTS],
              { supplier_id: consignment_supplier_id }
            )

            if access_context[:access_token]&.guest
              raise GoshPosh::Platform::Errors::UnableToCreateConsignmentRequestError.new(
                GoshPosh::Platform::Errors::ConsignmentErrorMessages::OPEN_CONSIGNMENT_ADDRESS_ERROR
              )
            else
              raise GoshPosh::Platform::Errors::UnableToCreateConsignmentRequestError
            end
          end
        end

        max_open_consignments = if access_context[:access_token]&.guest
                                  consignment_fs[:max_open_consignments_per_guest_supplier]
                                else
                                  consignment_fs[:max_open_consignments_per_supplier]
                                end
        open_requests_count = get_open_requests_count(consignment_supplier_id)
        if max_open_consignments.present? && open_requests_count >= max_open_consignments
          raise GoshPosh::Platform::Errors::UnableToCreateConsignmentRequestError
        end
      end
    end

    def post_consignment_ops_update_to_slack(summary_message_components)
      message = summary_message_components.join("\n")
      GoshPosh::Platform::Util.post_to_slack(GoshPosh::Platform::SlackAlertTopics::CONSIGNMENT_OPS_ALERTS,
                                             message,
                                             message,
                                             'category: consignments')
    end

    def handle_consignment_address_update_for_hard_matched_crs(
      access_context,
      user_id,
      old_address,
      new_address,
      radius_type: :default
    )

      user_home_domain = get_user_home_domain(access_context, user_id)
      access_context[:home_domain] ||= user_home_domain
      
      consignment_schedule_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_SCHEDULE,
        user_home_domain
      )

      consignment_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT,
        user_home_domain
      )

      case radius_type
      when :default
        search_radius = consignment_schedule_fs[:supplier_enrollment_partner_search_radius_miles]
      when :extended
        search_radius = consignment_schedule_fs[:supplier_enrollment_extended_partner_search_radius_miles]
      when :uber_default
        search_radius = consignment_schedule_fs[:uber_default_partner_search_radius_miles]
      else
        raise GoshPosh::Platform::Errors::InvalidInputError.new('Invalid radius type')
      end


      max_open_consignments_with_same_address = consignment_fs[:max_open_consignments_with_same_address]
      user = services.user_service.by_id(user_id)

      if user[:consignment_partner]
        hard_matched_crs = find_open_consignment_requests_by_partner_address(
          old_address, max_open_consignments_with_same_address
        )
      else
        hard_matched_crs = find_open_consignment_requests_by_address(
          old_address, max_open_consignments_with_same_address
        )
      end
      return if hard_matched_crs.empty?

      hard_matched_crs.each do |consignment_request|
        update_partner_address = user[:consignment_partner]

        # No partner assigned → just update address and move on
        if consignment_request[:consignment_partner_id].nil?
          services.consignment_service.update_consignment_request_on_address_change(
            consignment_request[:id],
            new_address.deep_symbolize_keys,
            GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_OPEN_REQUESTS_STATES,
            false,
            update_partner_address
          )

          update_consignment_request_index(consignment_request[:id])
          next
        end

        within_radius = within_radius_for_address_update?(
          consignment_request,
          new_address,
          search_radius,
          update_partner_address
        )

        begin
          if within_radius
            handle_address_update_within_radius(access_context, consignment_request, new_address, update_partner_address)
          else
            handle_address_update_outside_radius(access_context, consignment_request, new_address, update_partner_address)
          end
        rescue => error
          services.logger.error(
            GoshPosh::Platform::Util.print_stack_trace(
              "#{__method__} - Failed for CR #{consignment_request[:id]}", error
            )
          )
        end
      end
    end

    def handle_address_update_within_radius(access_context, consignment_request, new_address, update_partner_address)
      consignment_request = services.consignment_service.update_consignment_request_on_address_change(
        consignment_request[:id],
        new_address.deep_symbolize_keys,
        GoshPosh::Platform::Consignments::ConsignmentRequestState::SUPPLIER_OPEN_REQUESTS_STATES,
        false,
        update_partner_address
      )
      update_consignment_request_index(consignment_request[:id])

      shipping_label = services.shipping_service.get_latest_consignment_shipping_label(consignment_request[:id])
      refund_shipping_label(access_context, shipping_label[:id])

      partner_id = consignment_request[:consignment_partner_id]
      pickup_week_number = consignment_week_number(
        consignment_request[:supplier_shipment_collection_info][:pickup_start_at]
      )
      partner_consignment_address = services.order_service.default_consignment_address_in_user_address_list(
        partner_id
      )

      consignment_partner_info = services.consignment_service.consignment_partner_info(
        partner_id
      )

      partner_consignment_address[:phone] = services.user_service.get_unmasked_phone_number_for_user(partner_id)
      unless partner_consignment_address[:phone]
        services.logger.warn "#{__method__} CR Id: #{consignment_request[:id]}, Partner without phone number: #{partner_id}"
        return
      end

      quote = nil
      begin
        quote = services.shipping_service.generate_delivery_quote(
          GoshPosh::Platform::Commerce::ShippingCarrier::UBER,
          partner_consignment_address,
          consignment_request[:consignment_supplier_address]
        )
      rescue => error
        services.logger.error GoshPosh::Platform::Util.print_stack_trace(
          "#{__method__} Uber Quote error: #{consignment_request[:id]}", error
        )
      end

      unless quote && quote[:success]
        services.logger.warn "#{__method__} CR Id: #{consignment_request[:id]}, Partner Id: #{partner_id}, Uber Quote unsuccessful: #{quote}"
        return
      end

      generated_label = nil
      begin
        generated_label = generate_consignment_shipping_label(
          consignment_request,
          consignment_request[:consignment_supplier_id],
          GoshPosh::Platform::Commerce::ShippingLabelReason::COMPLETE_CONSIGNMENT_REQUEST,
          services,
          {
            shipping_address: partner_consignment_address,
            dropoff_notes: consignment_partner_info[:address_notes],
            quote_id: quote[:quote_id]
          }
        )
      rescue => error
        services.logger.error GoshPosh::Platform::Util.print_stack_trace(
          "#{__method__} Uber label generation error: CR_ID: #{consignment_request[:id]}, partner_id: #{partner_id}", error
        )
      end

      return if generated_label

      services.logger.warn(
        "#{__method__} CR Id: #{consignment_request[:id]}, Partner Id: #{partner_id}, Label generation failed"
      )

      un_reserve_partner_capacity_for_hard_matching(partner_id, pickup_week_number)
    end

    def handle_address_update_outside_radius(
      access_context,
      consignment_request,
      new_address,
      update_partner_address
    )
      begin
        consignment_request =
          cancel_hard_matched_consignment_request(consignment_request[:id])
      rescue => error
        services.logger.error(
          GoshPosh::Platform::Util.print_stack_trace(
            "#{__method__} Failed to cancel the consignment request: #{consignment_request[:id]}",
            error
          )
        )
        return
      end

      consignment_schedule_fs = GoshPosh::FeatureSettings.get_domain_based(
        GoshPosh::Platform::FeatureSettings::Feature::CONSIGNMENT_SCHEDULE,
        access_context[:home_domain]
      )

      return unless consignment_schedule_fs[:quick_partner_matching]

      begin
        recreate_consignment_request_for_quick_matching(
          access_context,
          consignment_request,
          new_address: new_address,
          update_partner_address: update_partner_address
        )
      rescue => error
        services.logger.error(
          GoshPosh::Platform::Util.print_stack_trace(
            "#{__method__} Failed to recreate consignment_request: #{consignment_request[:id]}",
            error
          )
        )
      end
    end
  end
end

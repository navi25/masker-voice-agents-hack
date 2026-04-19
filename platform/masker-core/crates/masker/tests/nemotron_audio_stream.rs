//! Audio-stream regression cases inspired by the NVIDIA Nemotron-PII dataset:
//! synthetic, span-labeled examples across many domains such as healthcare,
//! identity verification, education, logistics, legal, retail, and finance.
//!
//! We adapt those document-style examples into spoken utterances so the tests
//! exercise Masker's streaming pipeline shape:
//! STT -> detection -> policy -> masking -> route -> local audio output.

use masker::contracts::RiskLevel;
use masker::{AudioChunk, AudioChunkResult, EntityType, Route, StreamingPipeline, TraceStage};

fn run_stream_case(text: &str) -> AudioChunkResult {
    let pipeline = StreamingPipeline::new_with_defaults();
    let chunk = AudioChunk {
        seq: 0,
        data: text.as_bytes().to_vec(),
        source_path: None,
        sample_rate: 16_000,
        duration_ms: 1_000,
    };

    pipeline
        .process("ses_nemotron_audio", None, &chunk)
        .expect("stream case should process successfully")
}

fn assert_trace_shape(result: &AudioChunkResult) {
    let stages: Vec<_> = result.trace.iter().map(|event| event.stage).collect();
    assert!(stages.contains(&TraceStage::Stt));
    assert!(stages.contains(&TraceStage::Detection));
    assert!(stages.contains(&TraceStage::Policy));
    assert!(stages.contains(&TraceStage::Masking));
    assert!(stages.contains(&TraceStage::Routing));
}

fn assert_audio_contract(result: &AudioChunkResult) {
    match result.route {
        Route::SafeToSend => assert_eq!(result.audio_out, result.raw_transcript.as_bytes()),
        Route::MaskedSend | Route::LocalOnly => {
            assert_eq!(result.audio_out, result.masked_transcript.as_bytes())
        }
    }
}

fn assert_entity_types(result: &AudioChunkResult, expected: &[EntityType]) {
    let actual: Vec<_> = result
        .detection
        .entities
        .iter()
        .map(|entity| entity.kind)
        .collect();
    for entity in expected {
        assert!(
            actual.contains(entity),
            "expected entity {:?} in {:?}",
            entity,
            actual
        );
    }
}

macro_rules! nemotron_stream_test {
    (
        $name:ident,
        text: $text:expr,
        route: $route:expr,
        risk: $risk:expr,
        entities: [$($entity:expr),* $(,)?],
        masked_excludes: [$($masked_exclude:expr),* $(,)?]
    ) => {
        #[test]
        fn $name() {
            let result = run_stream_case($text);

            assert_eq!(result.route, $route);
            assert_eq!(result.detection.risk_level, $risk);
            assert_entity_types(&result, &[$($entity),*]);
            $(
                assert!(
                    !result.masked_transcript.contains($masked_exclude),
                    "masked transcript still contains {:?}: {}",
                    $masked_exclude,
                    result.masked_transcript
                );
            )*
            if !matches!($route, Route::SafeToSend) {
                assert_ne!(result.masked_transcript, result.raw_transcript);
            }
            assert_trace_shape(&result);
            assert_audio_contract(&result);
        }
    };
}

nemotron_stream_test!(
    financial_services_application_masks_dob_and_address,
    text: "I'm applying for a financial services account. My date of birth is 05/22/1987 and I live at 87 Mission Street.",
    route: Route::MaskedSend,
    risk: RiskLevel::Medium,
    entities: [EntityType::Dob, EntityType::Address],
    masked_excludes: ["05/22/1987", "87 Mission Street"]
);

nemotron_stream_test!(
    blood_donor_registration_masks_dob_from_healthcare_form,
    text: "For the blood donor registration form, my date of birth is 05/25/1961 and my blood type is O positive.",
    route: Route::MaskedSend,
    risk: RiskLevel::Medium,
    entities: [EntityType::Dob],
    masked_excludes: ["05/25/1961"]
);

nemotron_stream_test!(
    logistics_bill_of_lading_with_tracking_url_is_safe,
    text: "This bill of lading covers vehicle VF3FJ7X14G0001234 and the tracking URL is https://dhl.com/tracking?trackingId=12345.",
    route: Route::SafeToSend,
    risk: RiskLevel::None,
    entities: [],
    masked_excludes: []
);

nemotron_stream_test!(
    emergency_contact_form_masks_phone_and_dob,
    text: "For the emergency contact form, Maria was born on 03/05/2002 and her phone number is 707-859-9753.",
    route: Route::MaskedSend,
    risk: RiskLevel::Medium,
    entities: [EntityType::Dob, EntityType::Phone],
    masked_excludes: ["03/05/2002", "707-859-9753"]
);

nemotron_stream_test!(
    legal_cover_page_masks_email_address,
    text: "On the legal correspondence cover page, send status updates to boycec1971@gmail.com.",
    route: Route::MaskedSend,
    risk: RiskLevel::Medium,
    entities: [EntityType::Email],
    masked_excludes: ["boycec1971@gmail.com"]
);

nemotron_stream_test!(
    biotechnology_lab_report_without_direct_identifiers_is_safe,
    text: "On 2023-08-15 at 10:30 AM VitaGenesis Biotechnologies uploaded the lab report to https://biotechinnovations.com/reports/lab-report-2024.",
    route: Route::SafeToSend,
    risk: RiskLevel::None,
    entities: [],
    masked_excludes: []
);

nemotron_stream_test!(
    student_resume_masks_phone_and_email,
    text: "My student resume says you can reach me at 931-608-0499 or raulr1968@hotmail.com.",
    route: Route::MaskedSend,
    risk: RiskLevel::Medium,
    entities: [EntityType::Phone, EntityType::Email],
    masked_excludes: ["931-608-0499", "raulr1968@hotmail.com"]
);

nemotron_stream_test!(
    student_financial_aid_application_routes_ssn_local_only,
    text: "For student financial aid verification, my social security number is 250-38-8116.",
    route: Route::LocalOnly,
    risk: RiskLevel::High,
    entities: [EntityType::Ssn],
    masked_excludes: ["250-38-8116"]
);

nemotron_stream_test!(
    surgical_report_routes_mrn_local_only,
    text: "For the surgical report, patient MRN 99812 needs follow-up after surgery.",
    route: Route::LocalOnly,
    risk: RiskLevel::High,
    entities: [EntityType::Mrn, EntityType::HealthContext],
    masked_excludes: ["99812"]
);

nemotron_stream_test!(
    prior_authorization_masks_insurance_id_with_health_context,
    text: "My insurance ID is BCBS-887421 and I have chest pain.",
    route: Route::MaskedSend,
    risk: RiskLevel::High,
    entities: [EntityType::InsuranceId, EntityType::HealthContext],
    masked_excludes: ["BCBS-887421"]
);

nemotron_stream_test!(
    pharmacy_refill_masks_phone_and_address,
    text: "For my prescription refill, call me at 415-555-0123 because I live at 4821 Mission Street.",
    route: Route::MaskedSend,
    risk: RiskLevel::High,
    entities: [EntityType::HealthContext, EntityType::Phone, EntityType::Address],
    masked_excludes: ["415-555-0123", "4821 Mission Street"]
);

nemotron_stream_test!(
    radiology_followup_with_health_context_only_stays_safe,
    text: "I have chest pain and want to discuss my MRI follow-up.",
    route: Route::SafeToSend,
    risk: RiskLevel::Low,
    entities: [EntityType::HealthContext],
    masked_excludes: []
);

nemotron_stream_test!(
    onboarding_workflow_masks_email_from_identity_verification,
    text: "For identity verification onboarding, send the paperwork to priya.redwood@example.com.",
    route: Route::MaskedSend,
    risk: RiskLevel::Medium,
    entities: [EntityType::Email],
    masked_excludes: ["priya.redwood@example.com"]
);

nemotron_stream_test!(
    spoken_ssn_from_tax_document_routes_local_only,
    text: "For my tax document, my SSN is one two three four five six seven eight nine.",
    route: Route::LocalOnly,
    risk: RiskLevel::High,
    entities: [EntityType::Ssn],
    masked_excludes: ["one two three four five six seven eight nine"]
);

nemotron_stream_test!(
    payment_application_masks_credit_card_number,
    text: "For the payment application, my credit card number is 4285946 1234.",
    route: Route::MaskedSend,
    risk: RiskLevel::Medium,
    entities: [EntityType::Other],
    masked_excludes: ["4285946 1234"]
);

nemotron_stream_test!(
    payment_application_masks_cvv_number,
    text: "The C V V number is 123.",
    route: Route::MaskedSend,
    risk: RiskLevel::Medium,
    entities: [EntityType::Other],
    masked_excludes: ["123"]
);

nemotron_stream_test!(
    building_permit_masks_contextual_address,
    text: "For the building permit, I stay at 108169PL SW.",
    route: Route::MaskedSend,
    risk: RiskLevel::Medium,
    entities: [EntityType::Address],
    masked_excludes: ["108169PL SW"]
);

nemotron_stream_test!(
    telehealth_intake_masks_identifiers_but_keeps_health_context,
    text: "During telehealth intake I mentioned asthma, my phone is 212-555-0148, and my email is dana.patient@example.org.",
    route: Route::MaskedSend,
    risk: RiskLevel::High,
    entities: [EntityType::HealthContext, EntityType::Phone, EntityType::Email],
    masked_excludes: ["212-555-0148", "dana.patient@example.org"]
);

nemotron_stream_test!(
    sales_invoice_without_personal_data_is_safe,
    text: "The sales invoice lists order 48392, subtotal 219 dollars, tax 18 dollars, and total 237 dollars.",
    route: Route::SafeToSend,
    risk: RiskLevel::None,
    entities: [],
    masked_excludes: []
);

nemotron_stream_test!(
    cybersecurity_change_request_masks_email_but_keeps_device_details_safe,
    text: "For the MAC address change request, contact alex.ops@example.com about device 00:1B:44:11:3A:B7.",
    route: Route::MaskedSend,
    risk: RiskLevel::Medium,
    entities: [EntityType::Email],
    masked_excludes: ["alex.ops@example.com"]
);

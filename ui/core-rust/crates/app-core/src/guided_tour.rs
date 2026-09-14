// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The tour's script and demonstration state are shared by both clients.
use app_ui_contracts::tour::{
    UiGuidedTour, UiTourAction, UiTourMapPoint, UiTourPage, UiTourPlacement, UiTourPresentation,
    UiTourShortcut, UiTourSurface, UiTourViewport,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Demo {
    Empty,
    Primary,
    Activated,
    Waypoints,
    AirwayEndpoints,
    AirwayEntry,
    AirwayExit,
    AirwayAdded,
    RouteEndpoints,
    RouteDraft,
    RouteVia,
    RouteApplied,
    Cardinal,
    Estimated,
    Preview,
    PreviewForward,
    Spot,
}

pub(crate) struct Step {
    pub id: &'static str,
    pub chapter: &'static str,
    pub title: &'static str,
    pub body: &'static str,
    pub placement: UiTourPlacement,
    pub presentation: UiTourPresentation,
    pub page: UiTourPage,
    pub surface: UiTourSurface,
    pub subject: &'static str,
    pub targets: &'static [&'static str],
    pub demo: Demo,
    pub android_only: bool,
    pub web_only: bool,
}

pub(crate) fn steps(android: bool) -> Vec<Step> {
    use Demo::*;
    use UiTourPage::*;
    use UiTourSurface as S;
    let mut result = Vec::new();
    let mut current_chapter = "Primary functions";
    macro_rules! step {
        ($id:expr, $title:expr, $body:expr, $page:ident, $surface:ident, $subject:expr, $demo:ident, [$($target:expr),*]) => {
            result.push(Step { id: $id, chapter: current_chapter, title: $title, body: $body,
                placement: UiTourPlacement::Auto, presentation: UiTourPresentation::Callout,
                page: $page, surface: S::$surface, subject: $subject, demo: $demo,
                targets: &[$($target),*], android_only: false, web_only: false });
        }
    }
    macro_rules! chapter {
        ($id:expr, $title:expr, $demo:ident) => {
            current_chapter = $title;
            step!($id, $title, "", Home, None, "", $demo, []);
            result.last_mut().unwrap().presentation = UiTourPresentation::TitleCard;
            result.last_mut().unwrap().placement = UiTourPlacement::Center;
        };
    }
    step!("welcome", "Three pages for most of your flying", "Chart, Flight Plan and Plates are Aerobag’s main pages. This tour operates the app for you: use Next and Back, or close it at any time. Your own plan and settings will be restored when you leave.", Map, None, "", Empty, ["chart-plate", "cdi"]);
    step!(
        "chart",
        "The Chart page",
        "Sectionals, TACs and IFR charts share this page with useful overlays and flight data.",
        Map,
        None,
        "",
        Empty,
        ["base-map", "layers"]
    );
    step!(
        "base-map",
        "Select the base map",
        "Choose a sectional, TAC, IFR chart or shaded relief here.",
        Map,
        BaseMap,
        "",
        Empty,
        ["base-map-menu"]
    );
    step!(
        "layers",
        "Choose overlays",
        "Add weather, navigation features, airspace and other overlays with these controls.",
        Map,
        Layers,
        "",
        Empty,
        ["layers-menu"]
    );
    step!(
        "cdi",
        "The CDI opens Flight Plan",
        "The course guidance display is also a shortcut to your flight plan. Next takes you there.",
        Map,
        None,
        "",
        Empty,
        ["cdi"]
    );
    step!(
        "enter-primary",
        "Enter a flight plan",
        "To enter a flight plan, you type waypoints into this route field. Here we’ve typed KRNT SEA OLM UBG KUAO for you. Next adds them to the plan.",
        FlightPlan,
        RouteEntry,
        "KRNT SEA OLM UBG KUAO",
        Empty,
        ["route-entry"]
    );
    step!(
        "primary-plan",
        "Your route, one row at a time",
        "The waypoints now form a flight plan. Select SEA to open its action tray.",
        FlightPlan,
        FlightPlanRow,
        "SEA",
        Primary,
        ["plan-row", "plan-row-activate"]
    );
    step!("activate", "Activate course guidance", "SEA is now active. The CDI shows guidance for the active leg; you can tap it from other pages to return here.", FlightPlan, None, "", Activated, ["cdi"]);
    step!(
        "chart-return",
        "Return to the Chart",
        "The Chart/Plate button brings you back to the chart. Next demonstrates that tap.",
        FlightPlan,
        None,
        "",
        Activated,
        ["chart-plate"]
    );
    step!("chart-plate-toggle", "Tap again to switch to Plates", "Now you’re back on the Chart. Tap the same button again to switch to Plates. Next demonstrates that second tap.", Map, None, "", Activated, ["chart-plate"]);
    step!(
        "plate-airport",
        "Choose the arrival airport",
        "The airport menu includes airports from your flight plan. Choose KUAO to see its plates.",
        Charts,
        PlateAirports,
        "KUAO",
        Activated,
        ["plate-airport-menu"]
    );
    step!("plate-folder", "Browse the plate folder", "FLDR shows approach plates, arrival and departure procedures, and the airport diagram. Next opens KUAO’s diagram.", Charts, PlateFolder, "KUAO", Activated, ["plate-folder", "plate-folder-content"]);
    step!("diagram", "Keep a plate at hand", "This is KUAO’s airport diagram. The Chart/Plate button lets you switch between the chart and the plate you’re using.", Charts, None, "KUAO:diagram", Activated, ["chart-plate"]);
    step!("three-pages", "Chart, Plates and Flight Plan", "You’ve seen the three places where you’ll spend most of your time. Everything else is under Home.", Map, None, "", Activated, ["home"]);
    step!("home", "Everything else starts here", "Home opens planning tools, offline packages, Cloud, settings and status. Guided Tour is here whenever you want to start again.", Home, None, "", Activated, ["home-guided-tour"]);
    chapter!("planning-title", "Flight Planning", Activated);
    step!("clear-plan", "Start a new plan", "The final waypoint’s Remove All Above action clears the flight plan. We’ll use a fresh example next.", FlightPlan, FlightPlanRow, "KUAO", Activated, ["plan-remove-all-above"]);
    step!("enter-waypoints", "Type your waypoints", "You can type named waypoints to describe a route you already have in mind. Here we’ve entered KRNT BANDR ELN YKM S40; Next shows that route on the chart.", FlightPlan, RouteEntry, "KRNT BANDR ELN YKM S40", Empty, ["route-entry"]);
    step!(
        "waypoints-map",
        "See the route on the Chart",
        "The flight plan appears on the map, making its path easy to inspect.",
        Map,
        None,
        "",
        Waypoints,
        ["map"]
    );
    step!("airway-start", "Enter a published airway", "Start with KRNT S40. Select KRNT, then Add Airway to choose an airway between the endpoints.", FlightPlan, FlightPlanRow, "KRNT", AirwayEndpoints, ["plan-add-airway"]);
    step!(
        "airway-entry",
        "Choose where to join",
        "The airway picker suggests nearby entry points. We’ll join at SEA.",
        FlightPlan,
        None,
        "SEA",
        AirwayEntry,
        ["airway-picker"]
    );
    step!("airway-exit", "Choose where to leave", "BRUKK is suggested as the nearest exit to S40; select it. Aerobag retains the airway as a section of the plan.", FlightPlan, None, "BRUKK", AirwayExit, ["airway-picker"]);
    step!(
        "airway-added",
        "The airway is in the plan",
        "The airway section connects SEA to BRUKK. Its fixes appear in the plan, with the airway name identifying the published route between them.",
        FlightPlan,
        None,
        "",
        AirwayAdded,
        ["flight-plan"]
    );
    step!("find-start", "Let Aerobag find a route", "Start with KRNT KMWC. Select KRNT and Find Route to discover a path along published airways.", FlightPlan, FlightPlanRow, "KRNT", RouteEndpoints, ["plan-find-route"]);
    step!("find-route", "Compare the route’s altitude constraints", "Aerobag suggests an airway route. The highlighted altitude constraints help you see where the route climbs. VOR and GNSS select which navigation altitudes to show.", Map, None, "", RouteDraft, ["routing-summary", "routing-gnss"]);
    step!("route-via", "Drag the route around the mountains", "We’ve dragged the route through Rock Springs OCS VOR. The route is recomputed through that pin. Compare the highlighted altitude constraints; a different path may permit lower travel.", Map, None, "OCS", RouteVia, ["routing-route", "routing-apply"]);
    step!("route-apply", "Apply commits the route", "Apply replaces the selected interval of the flight plan. The map editor’s Undo and Redo change the draft; Flight Plan Undo can undo the applied route.", Map, None, "", RouteApplied, ["cdi"]);
    step!("altitude-entry", "Choose a cruising altitude", "The Estimate button opens Altitude Planner. Its table compares cruising altitudes using an airplane model and forecast conditions.", FlightPlan, None, "", RouteApplied, ["estimate"]);
    step!("aircraft-model", "Select an airplane model", "Choose the Cardinal from the airplane menu. The airplane model supplies performance estimates.", AltitudePlanner, AircraftModels, "Cardinal", RouteApplied, ["aircraft-models", "aircraft-model-option"]);
    step!("use-model", "Use the model for estimates", "Use Model selects the forecast winds and temperatures for the estimates. Fetch the latest forecast first when needed. Next uses the forecast and chooses 10,000 feet from the comparison table.", AltitudePlanner, None, "", Cardinal, ["use-model", "altitude-table"]);
    step!("estimated", "Blue values are model estimates", "The Estimate button now describes the selected conditions. Blue data columns identify values estimated from the airplane and wind models.", FlightPlan, None, "", Estimated, ["estimate", "flight-plan"]);
    step!("inspect", "Query the map", "Click a feature on the chart to inspect it. Here’s KMWC, our destination. The weather symbol opens its weather and NOTAMs.", Map, Inspector, "KMWC", Estimated, ["inspector", "inspector-weather"]);
    step!(
        "weather",
        "Read destination weather",
        "The weather panel brings together observations, forecasts and NOTAMs for the selected airport.",
        Map,
        Weather,
        "KMWC",
        Estimated,
        ["weather"]
    );
    step!("notams", "Review current NOTAMs", "Like everything in this software, weather and NOTAM data is not guaranteed to be complete or correct. Always get a briefing from official sources.", Map, Notams, "KMWC", Estimated, ["notams"]);
    step!(
        "airport-info",
        "A quick look at the airport",
        "Info collects pattern elevations, runways, frequencies and sunset time.",
        Map,
        AirportInfo,
        "KMWC",
        Estimated,
        ["airport-info"]
    );
    step!(
        "supplement",
        "Chart Supplement and plates",
        "The airport inspector links directly to the Chart Supplement and the airport’s plates.",
        Map,
        Inspector,
        "KMWC",
        Estimated,
        ["inspector-supplement", "inspector-plates"]
    );
    step!("approach", "Brief your approach", "Here’s KMWC’s RNAV 15L approach. Use FLDR to choose another plate and Chart/Plate to switch back to the chart.", Charts, None, "KMWC:RNAV 15L", Estimated, ["plate-folder", "chart-plate"]);
    chapter!("in-flight-title", "In Flight", Estimated);
    step!("simulator", "Flying with Aerobag", "Aerobag makes a great companion for simulator flights. Use it in a real plane at your own risk. Not for navigation.", Map, None, "", Estimated, []);
    step!("ownship", "Ownship control", "The Ownship button chooses GPS or Plan Preview. GPS can be paused to save battery while you’re at lunch.", Map, Ownship, "", Estimated, ["ownship-menu"]);
    step!("preview", "Preview your plan", "Plan Preview moves the ownship along your flight plan. CTR keeps it in view. The forward buttons move along the leg or skip to the next waypoint; their labels also show the keyboard shortcuts.", Map, Ownship, "", Preview, ["plan-preview", "center"]);
    step!("preview-forward", "Move along the route", "The preview has moved forward along the plan. Use the smaller steps to inspect a leg or the skip buttons to reach a waypoint.", Map, Ownship, "", PreviewForward, ["preview-controls"]);
    step!("orientation", "North-up or track-up", "N keeps raster-chart text upright. TRK rotates the map with the aircraft’s track, which can be useful while moving.", Map, None, "", PreviewForward, ["orientation"]);
    step!("spot", "Inspect any spot", "We’ve selected a spot near Enumclaw, ahead of the aircraft. SPOT gives you elevation and distance for that point. +INSERT adds that spot to your flight plan.", Map, Inspector, "SPOT", PreviewForward, ["inspector-spot", "map-spot-marker", "inspector-insert"]);
    step!(
        "spot-insert",
        "Add a spot to the plan",
        "That spot is now a waypoint in the demonstration plan.",
        FlightPlan,
        None,
        "",
        Spot,
        ["flight-plan"]
    );
    chapter!("offline-title", "Offline Packages", Estimated);
    step!("offline-web", "Take Aerobag with you", "Install the Android APK from About to take Aerobag with you. On Android, you can download charts and other products for offline use.", Home, None, "", Estimated, ["home-about"]);
    result.last_mut().unwrap().web_only = true;
    step!("offline-regions", "Choose regions", "Select the regions you want to have available offline. We’ll use Northwest for this example. These are demonstration preferences; the tour won’t start downloads or delete your data.", OfflinePackages, OfflineRegions, "Northwest", Estimated, ["offline-regions"]);
    result.last_mut().unwrap().android_only = true;
    step!(
        "offline-play",
        "Play keeps a region up to date",
        "Play means: load this region now, and fetch updates whenever new data becomes available.",
        OfflinePackages,
        OfflineRegions,
        "play",
        Estimated,
        ["offline-northwest"]
    );
    result.last_mut().unwrap().android_only = true;
    step!(
        "offline-pause",
        "Pause keeps the current data",
        "Pause means: keep the data I already have, but don’t fetch new data for this region.",
        OfflinePackages,
        OfflineRegions,
        "pause",
        Estimated,
        ["offline-northwest"]
    );
    result.last_mut().unwrap().android_only = true;
    step!("offline-remove", "Remove frees space", "No means: remove the region’s data to free space. Other selected regions may still share some products.", OfflinePackages, OfflineRegions, "remove", Estimated, ["offline-northwest"]);
    result.last_mut().unwrap().android_only = true;
    step!("offline-map", "See the region boundaries", "With shaded relief and Offline Regions enabled, you can see which areas each region covers. CTR is off and the map is north-up for this overview.", Map, None, "regions", Estimated, ["map"]);
    result.last_mut().unwrap().android_only = true;
    step!("offline-products", "Choose the products you need", "Select the chart and data products for the kind of flying you do. The help buttons describe each product.", OfflinePackages, OfflineProducts, "", Estimated, ["offline-products", "offline-help"]);
    result.last_mut().unwrap().android_only = true;
    step!(
        "offline-help",
        "Help explains each layer",
        "Read a product’s description before deciding whether to download it.",
        OfflinePackages,
        OfflineHelp,
        "",
        Estimated,
        ["offline-help-panel"]
    );
    result.last_mut().unwrap().android_only = true;
    step!("offline-apply", "Apply your preferences", "Outside the tour, Apply Changes downloads and manages maps according to your choices. Here, Northwest is selected and the other regions are paused.", OfflinePackages, OfflineRegions, "apply", Estimated, ["offline-apply"]);
    result.last_mut().unwrap().android_only = true;
    chapter!("customizing-title", "Customizing Aerobag", Estimated);
    step!(
        "cloud-entry",
        "Connect your devices",
        "The Cloud button opens account setup and sync controls. Next takes you there.",
        Home,
        None,
        "",
        Estimated,
        ["home-cloud"]
    );
    step!("cloud", "Crossfill between devices", "Create an encrypted Cloud Sync Account and share it between your devices. Draw a flight plan on the web and have it appear on your tablet. Region and product preferences sync too, including to your backup tablet.", Cloud, None, "", Estimated, ["cloud-create"]);
    step!(
        "settings",
        "Fine tune Aerobag",
        "Settings controls the details of how Aerobag works and looks.",
        Home,
        None,
        "",
        Estimated,
        ["home-settings"]
    );
    step!(
        "status",
        "See how things are functioning",
        "Status describes Aerobag’s components and their data.",
        Home,
        None,
        "",
        Estimated,
        ["home-status"]
    );
    step!("finish", "Ready to explore", "You can reopen Guided Tour from Home whenever you like. Closing partway through remembers your place; Start over returns to the beginning. Finish restores your own plan and preferences.", Home, None, "", Estimated, ["home-guided-tour"]);
    for step in &mut result {
        step.placement = match step.id {
            "activate" | "airway-start" | "find-start" | "find-route" | "altitude-entry" => {
                UiTourPlacement::TopRight
            }
            "cloud" => UiTourPlacement::BottomRight,
            _ => step.placement,
        };
    }
    result
        .into_iter()
        .filter(|s| (!s.android_only || android) && (!s.web_only || !android))
        .collect()
}

pub(crate) const TOUR_SPOT: crate::LatLon = crate::LatLon {
    // A quieter point near Enumclaw, southeast of the preview aircraft.
    lat: 47.205,
    lon: -121.99,
};

pub(crate) fn view(index: usize, generation: u64, android: bool) -> UiGuidedTour {
    let all = steps(android);
    let step = &all[index];
    let (lat, lon, zoom) = match step.demo {
        Demo::Empty | Demo::Primary | Demo::Activated => (46.56, -122.64, 7.0),
        Demo::Waypoints
        | Demo::AirwayEndpoints
        | Demo::AirwayEntry
        | Demo::AirwayExit
        | Demo::AirwayAdded => (47.1, -121.0, 7.0),
        Demo::Preview | Demo::PreviewForward => (47.47, -122.20, 9.0),
        _ if step.page == UiTourPage::Map
            && matches!(
                step.surface,
                UiTourSurface::Inspector
                    | UiTourSurface::Weather
                    | UiTourSurface::Notams
                    | UiTourSurface::AirportInfo
            ) =>
        {
            (43.11, -88.035, 11.0)
        }
        _ => (43.4, -105.0, 4.6),
    };
    let viewport = if step.id == "spot" {
        UiTourViewport {
            // Leave room for the inspector to the left of the selected spot,
            // including on a portrait phone.
            lat: 47.28,
            lon: -122.34,
            zoom: 10.0,
            track_up: false,
            centered: false,
        }
    } else if step.id == "offline-map" {
        UiTourViewport {
            lat: 39.0,
            lon: -98.0,
            zoom: 3.5,
            track_up: false,
            centered: false,
        }
    } else {
        UiTourViewport {
            lat,
            lon,
            zoom,
            track_up: step.id == "orientation",
            centered: matches!(step.demo, Demo::Preview | Demo::PreviewForward),
        }
    };
    UiGuidedTour {
        generation,
        step_id: step.id.into(),
        chapter: step.chapter.into(),
        title: step.title.into(),
        body: step.body.into(),
        placement: step.placement,
        presentation: step.presentation,
        position: index as u32 + 1,
        total: all.len() as u32,
        page: step.page,
        surface: step.surface,
        subject: step.subject.into(),
        row_uid: None,
        option_uid: None,
        targets: step.targets.iter().map(|s| (*s).into()).collect(),
        viewport,
        map_point: (step.id == "spot").then_some(UiTourMapPoint {
            lat: TOUR_SPOT.lat,
            lon: TOUR_SPOT.lon,
        }),
        back_enabled: index > 0,
        next_label: if index + 1 == all.len() {
            "Finish"
        } else {
            "Next"
        }
        .into(),
        close_label: "Close tour".into(),
        restart_label: (index > 0).then(|| "Start over".into()),
        shortcuts: [
            ("enter", UiTourAction::Next),
            ("n", UiTourAction::Next),
            ("b", UiTourAction::Back),
        ]
        .into_iter()
        .map(|(key, action)| UiTourShortcut {
            key: key.into(),
            action,
        })
        .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guided_tour_platform_steps_and_navigation_are_complete() {
        for android in [false, true] {
            let steps = steps(android);
            let ids: std::collections::HashSet<_> = steps.iter().map(|s| s.id).collect();
            assert_eq!(ids.len(), steps.len());
            assert_eq!(ids.contains("offline-web"), !android);
            assert_eq!(ids.contains("offline-apply"), android);
            let mut section_title = steps[0].chapter;
            for (index, step) in steps.iter().enumerate() {
                if step.presentation == UiTourPresentation::TitleCard {
                    section_title = step.title;
                }
                assert_eq!(
                    step.chapter, section_title,
                    "{} chapter differs from its title card",
                    step.id
                );
                let view = view(index, 7, android);
                assert_eq!(view.step_id, step.id);
                if step.id == "spot" {
                    let point = view.map_point.as_ref().expect("SPOT geographic subject");
                    assert_eq!((point.lat, point.lon), (TOUR_SPOT.lat, TOUR_SPOT.lon));
                    assert!(
                        point.lon > view.viewport.lon,
                        "leave inspector space west of SPOT"
                    );
                } else {
                    assert!(view.map_point.is_none());
                }
                if view.presentation == UiTourPresentation::TitleCard {
                    assert!(view.body.is_empty());
                    assert!(view.targets.is_empty());
                    assert_eq!(view.placement, UiTourPlacement::Center);
                }
                assert_eq!(view.back_enabled, index > 0);
                assert_eq!(view.next_label == "Finish", index + 1 == steps.len());
            }
        }
    }
}

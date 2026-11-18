//! Reviewed installed library identities. Digests bind every Python module in
//! each reviewed pure wheel, including the package initializer.
use super::{ProviderPackage, ProviderSelection};

struct Pin {
    scope: &'static str,
    distribution: &'static str,
    version: &'static str,
    digest: &'static str,
    prerequisites: &'static [&'static str],
}

const PINS: &[Pin] = &[
    Pin {
        scope: "httpx",
        distribution: "httpx",
        version: "0.28.1",
        digest: "a6941f610e464e617429d9cd06adbe62138020eb0310cbadefd52e416dafa7ea",
        prerequisites: &["httpcore", "anyio"],
    },
    Pin {
        scope: "httpx2",
        distribution: "httpx2",
        version: "2.16.2",
        digest: "21f96b9e932dec8fb347ad8aa11bf3190852aa389a586d5aae09177edcd51539",
        prerequisites: &["httpcore2", "anyio"],
    },
    Pin {
        scope: "sqlalchemy",
        distribution: "sqlalchemy",
        version: "2.0.58",
        digest: "f4b23081c2666a2c26dad45c1da933d4603085a220a517b65fe827b5ca35f21e",
        prerequisites: &["aiosqlite"],
    },
    Pin {
        scope: "sqlmodel",
        distribution: "sqlmodel",
        version: "0.0.44",
        digest: "0f3fd981f0c9fd661ecd5edfa2125603ef4cd303fec80611563281377ba00c79",
        prerequisites: &["sqlalchemy", "aiosqlite"],
    },
    Pin {
        scope: "etlantic_sqlmodel",
        distribution: "etlantic-sqlmodel",
        version: "0.59.0",
        digest: "80204a5901c42fd8cc07dd3a75f9124dd18d661b2a1c8064e2ed392ebc4505ac",
        prerequisites: &["sqlmodel", "sqlalchemy", "etlantic"],
    },
    Pin {
        scope: "etlantic",
        distribution: "etlantic",
        version: "0.59.0",
        digest: "a57843b963f706582d29868746fe5794a49926efc8580e3f0d15b961c0153747",
        prerequisites: &[],
    },
    Pin {
        scope: "aiosqlite",
        distribution: "aiosqlite",
        version: "0.22.1",
        digest: "a00ccf4a991e8c0045390b5ca9350ea9f5d4a5f9fc27cabfcd77455c24f40a60",
        prerequisites: &[],
    },
    Pin {
        scope: "httpcore",
        distribution: "httpcore",
        version: "1.0.9",
        digest: "bbe1b6b2332cdec67f813fc2058167bd668c19df22c8ad4e9fef1204dfa7a935",
        prerequisites: &[],
    },
    Pin {
        scope: "httpcore2",
        distribution: "httpcore2",
        version: "2.16.2",
        digest: "c37e113a1b29561c7d3439d6b26fbd87cf81d20a5d77dc4e3680d6a1615505af",
        prerequisites: &[],
    },
    Pin {
        scope: "anyio",
        distribution: "anyio",
        version: "4.16.0",
        digest: "380452f086eda5aceaae0b59daef1b8b0ee4f068c94e53683bf912bf3dca6862",
        prerequisites: &[],
    },
];

fn pin(scope: &str) -> Option<&'static Pin> {
    PINS.iter().find(|pin| pin.scope == scope)
}

/// The fixed reviewed wheel source cap is independent of the analyzed project.
pub fn reviewed_library_source_limit(scope: &str) -> Option<usize> {
    pin(scope).map(|_| match scope {
        "sqlalchemy" => 10_000_000,
        "etlantic" => 6_000_000,
        _ => crate::DEFAULT_MAX_SOURCE_BYTES,
    })
}

pub fn reviewed_library_dependencies(scope: &str) -> &'static [&'static str] {
    pin(scope).map_or(&[], |pin| pin.prerequisites)
}

pub(crate) fn select(package: &ProviderPackage) -> ProviderSelection {
    let expected = pin(&package.import_scope).expect("reviewed library scope");
    let digest = package.digest();
    let reason =
        if package.distribution != expected.distribution || package.version != expected.version {
            Some(format!(
                "reviewed library requires {}=={}",
                expected.distribution, expected.version
            ))
        } else if digest != expected.digest {
            Some("selected installed source differs from the reviewed wheel".into())
        } else {
            None
        };
    ProviderSelection {
        import_scope: package.import_scope.clone(),
        distribution: package.distribution.clone(),
        version: package.version.clone(),
        artifact_sha256: digest,
        origin: "reviewed-library".into(),
        reason,
    }
}

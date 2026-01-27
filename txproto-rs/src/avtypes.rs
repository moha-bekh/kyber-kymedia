// Project Kyber: avtypes.rs
// Copyright © 2022-2026 Kyber SAS
// SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0
//
// This file is both under dual license: AGPLv3 and a Commercial one.
//
// ----
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::ffi::CString;
use std::ptr;

use txproto_sys as sys;

use crate::Result;

pub enum PixelFormat {
    None,
    NV12,
}

impl PixelFormat {
    pub(crate) fn to_sys(&self) -> sys::AVPixelFormat {
        match self {
            Self::None => sys::AVPixelFormat_AV_PIX_FMT_NONE,
            Self::NV12 => sys::AVPixelFormat_AV_PIX_FMT_NV12,
        }
    }
}

pub enum HWDeviceType {
    NONE,
    VDPAU,
    CUDA,
    VAAPI,
    DXVA2,
    QSV,
    VIDEOTOOLBOX,
    D3D11VA,
    DRM,
    OPENCL,
    MEDIACODEC,
    VULKAN,
}

impl HWDeviceType {
    pub(crate) fn to_sys(&self) -> sys::AVHWDeviceType {
        match self {
            Self::NONE => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_NONE,
            Self::VDPAU => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VDPAU,
            Self::CUDA => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_CUDA,
            Self::VAAPI => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VAAPI,
            Self::DXVA2 => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_DXVA2,
            Self::QSV => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_QSV,
            Self::VIDEOTOOLBOX => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            Self::D3D11VA => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_D3D11VA,
            Self::DRM => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_DRM,
            Self::OPENCL => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_OPENCL,
            Self::MEDIACODEC => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_MEDIACODEC,
            Self::VULKAN => sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VULKAN,
        }
    }
}

#[derive(Debug)]
pub struct Rational {
    value: sys::AVRational,
}

impl Rational {
    pub fn num(&self) -> i32 {
        self.value.num
    }

    pub fn den(&self) -> i32 {
        self.value.den
    }
}

impl From<sys::AVRational> for Rational {
    fn from(value: sys::AVRational) -> Self {
        Self { value }
    }
}

impl From<Rational> for f64 {
    fn from(v: Rational) -> f64 {
        v.value.num as f64 / v.value.den as f64
    }
}

#[derive(Debug)]
pub enum DictionaryValue {
    Bool(bool),
    Int(i64),
    String(String),
}

impl From<bool> for DictionaryValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for DictionaryValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<String> for DictionaryValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for DictionaryValue {
    fn from(value: &str) -> Self {
        Self::String(value.into())
    }
}

pub struct DictionaryEntry {
    pub key: String,
    pub value: DictionaryValue,
}

impl DictionaryEntry {
    pub fn new(key: String, value: DictionaryValue) -> Self {
        Self { key, value }
    }
}

impl<T> From<(&str, T)> for DictionaryEntry
where
    DictionaryValue: From<T>,
{
    fn from(value: (&str, T)) -> Self {
        Self {
            key: value.0.into(),
            value: value.1.into(),
        }
    }
}

pub struct Dictionary {
    d: *mut sys::AVDictionary,
}

impl Dictionary {
    pub fn new() -> Self {
        Self { d: ptr::null_mut() }
    }

    pub fn from_entries(entries: &[DictionaryEntry]) -> Result<Self> {
        let mut this = Self::new();

        for entry in entries {
            match &entry.value {
                DictionaryValue::Bool(b) => {
                    this.set_int(&entry.key, (*b).into())?;
                }
                DictionaryValue::Int(i) => {
                    this.set_int(&entry.key, *i)?;
                }
                DictionaryValue::String(s) => {
                    this.set_str(&entry.key, s)?;
                }
            }
        }

        Ok(this)
    }

    pub fn take(self) -> *mut sys::AVDictionary {
        self.d
    }

    pub fn set_int(&mut self, name: &str, v: i64) -> Result<()> {
        let name = CString::new(name)?;

        let ret = unsafe { sys::av_dict_set_int(&mut self.d as *mut _, name.as_ptr(), v, 0) };
        if ret != 0 {
            return Err(ret.into());
        }

        Ok(())
    }

    pub fn set_str(&mut self, name: &str, v: &str) -> Result<()> {
        let name = CString::new(name)?;
        let v = CString::new(v)?;

        let ret = unsafe { sys::av_dict_set(&mut self.d as *mut _, name.as_ptr(), v.as_ptr(), 0) };
        if ret != 0 {
            return Err(ret.into());
        }

        Ok(())
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}
